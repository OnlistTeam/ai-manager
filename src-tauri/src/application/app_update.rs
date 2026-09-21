//! Signed product-update orchestration.
//!
//! A check starts in the background and returns immediately. The verified
//! package remains in memory until the user explicitly chooses to restart,
//! matching the low-interruption update flow used by developer tools.

mod channel;
mod retry;

use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime};

use tauri_plugin_opener::OpenerExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::domain::{AppError, AppUpdatePhase, AppUpdateStatus, ErrorCode};
use crate::platform::redact::redact_secrets;

use self::channel::{trusted_update_channel, UpdateChannelConfig, PRODUCT_DOWNLOAD_PAGE};
use self::retry::{backoff_after, endpoint_order, retryable_updater_error, MAX_ATTEMPTS};

/// How stale an "up to date" answer may get before a passive consumer is
/// allowed to ask again. A window left open for days would otherwise never
/// see a release published after launch, because the first conclusion is
/// terminal. It matches the renderer's `staleTime`, so the query that goes
/// stale is the one that reaches a backend willing to answer.
const RECHECK_AFTER: Duration = Duration::from_secs(6 * 60 * 60);

struct PreparedUpdate {
    update: Update,
    bytes: Vec<u8>,
}

pub struct AppUpdateManager {
    status: RwLock<AppUpdateStatus>,
    prepared: Mutex<Option<PreparedUpdate>>,
    channel: Option<UpdateChannelConfig>,
    /// When the last check concluded that nothing newer exists.
    up_to_date_at: Mutex<Option<SystemTime>>,
}

impl AppUpdateManager {
    pub fn new(app: &tauri::AppHandle) -> Self {
        let current_version = app.package_info().version.to_string();
        let channel = trusted_update_channel(app);
        Self {
            status: RwLock::new(AppUpdateStatus::new(
                current_version,
                channel.is_some(),
                MAX_ATTEMPTS,
            )),
            prepared: Mutex::new(None),
            channel,
            up_to_date_at: Mutex::new(None),
        }
    }

    pub fn status(&self) -> AppUpdateStatus {
        self.status
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Starts one background check/download. Repeated startup consumers share
    /// the same state; `force` only restarts a terminal check, never a download
    /// that is already active or ready to install.
    pub fn start(self: &Arc<Self>, app: tauri::AppHandle, force: bool) -> AppUpdateStatus {
        let up_to_date_age = self.up_to_date_age();
        {
            let mut status = self
                .status
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if !status.channel_ready {
                status.phase = AppUpdatePhase::Unconfigured;
                return status.clone();
            }
            if !should_start_check(status.phase, force, up_to_date_age) {
                return status.clone();
            }
            status.phase = AppUpdatePhase::Checking;
            status.available_version = None;
            status.downloaded_bytes = 0;
            status.total_bytes = None;
            status.attempt = 1;
        }
        self.prepared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();

        let manager = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            manager.check_and_download(app).await;
        });
        self.status()
    }

    pub fn open_download_page(&self, app: &tauri::AppHandle) -> Result<bool, AppError> {
        app.opener()
            .open_url(PRODUCT_DOWNLOAD_PAGE, None::<String>)
            .map_err(|error| {
                AppError::new(
                    ErrorCode::UpdateFailed,
                    "error.appUpdate.openDownloadFailed",
                )
                .with_technical(update_error_detail(&error))
                .with_remediation("error.remediation.retryOrViewDetails")
            })?;
        Ok(true)
    }

    pub async fn install_and_restart(&self, app: tauri::AppHandle) -> Result<bool, AppError> {
        let prepared = self
            .prepared
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .ok_or_else(|| {
                AppError::new(ErrorCode::UpdateFailed, "error.appUpdate.notReady")
                    .with_technical("no verified product update is waiting in memory")
                    .with_remediation("error.remediation.retryOrViewDetails")
            })?;

        #[cfg(target_os = "windows")]
        {
            // The Windows updater exits inside install(). Cleanup must happen
            // before that call or the tray icon and live takeover can be left
            // behind by the old process.
            crate::compat::ccswitch::app_update::prepare_for_update_install(&app).await;
            prepared
                .update
                .install(&prepared.bytes)
                .map_err(|error| install_error(error.to_string()))?;
            Ok(true)
        }

        #[cfg(not(target_os = "windows"))]
        {
            if let Err(error) = prepared.update.install(&prepared.bytes) {
                self.prepared
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .replace(prepared);
                return Err(install_error(error.to_string()));
            }

            crate::compat::ccswitch::app_update::restart_after_update_install(&app).await;
        }
    }

    async fn check_and_download(self: Arc<Self>, app: tauri::AppHandle) {
        let Some(channel) = self.channel.clone() else {
            self.fail("trusted update channel disappeared".to_string());
            return;
        };
        let mut expected_version: Option<String> = None;
        let mut attempt = 1_u8;

        loop {
            let updater = match self.build_updater(&app, &channel, attempt) {
                Ok(updater) => updater,
                Err(error) => {
                    self.fail(format!(
                        "failed to initialize updater: {}",
                        update_error_detail(&error)
                    ));
                    return;
                }
            };
            let mut update = match updater.check().await {
                Ok(Some(update)) => update,
                Ok(None) => {
                    if let Err(technical) = empty_check_outcome(expected_version.as_deref()) {
                        self.fail(technical.to_string());
                        return;
                    }
                    self.mark_up_to_date();
                    return;
                }
                Err(error) => {
                    if attempt < MAX_ATTEMPTS && retryable_updater_error(&error) {
                        self.wait_before_retry(attempt, "check", &error).await;
                        attempt += 1;
                        continue;
                    }
                    self.fail(format!(
                        "signed update check failed: {}",
                        update_error_detail(&error)
                    ));
                    return;
                }
            };

            if expected_version
                .as_ref()
                .is_some_and(|expected| expected != &update.version)
            {
                self.fail("signed update version changed between automatic retries".to_string());
                return;
            }
            expected_version.get_or_insert_with(|| update.version.clone());
            update.timeout = Some(Duration::from_secs(10 * 60));
            let version = update.version.clone();
            self.set_status(|status| {
                status.phase = AppUpdatePhase::Downloading;
                status.available_version = Some(version.clone());
                status.downloaded_bytes = 0;
                status.total_bytes = None;
                status.attempt = attempt;
            });

            let manager = Arc::clone(&self);
            let mut downloaded = 0_u64;
            match update
                .download(
                    move |chunk_len, total| {
                        downloaded = downloaded.saturating_add(chunk_len as u64);
                        manager.set_progress(downloaded, total);
                    },
                    || {},
                )
                .await
            {
                Ok(bytes) => {
                    self.prepared
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .replace(PreparedUpdate { update, bytes });
                    self.set_status(|status| {
                        status.phase = AppUpdatePhase::Ready;
                        if status.total_bytes.is_none() {
                            status.total_bytes = Some(status.downloaded_bytes);
                        }
                    });
                    log::info!("signed product update {version} is ready for restart");
                    return;
                }
                Err(error) => {
                    if attempt < MAX_ATTEMPTS && retryable_updater_error(&error) {
                        self.wait_before_retry(attempt, "download", &error).await;
                        attempt += 1;
                        continue;
                    }
                    self.fail(format!(
                        "signed update download or verification failed: {}",
                        update_error_detail(&error)
                    ));
                    return;
                }
            }
        }
    }

    fn build_updater(
        &self,
        app: &tauri::AppHandle,
        channel: &UpdateChannelConfig,
        attempt: u8,
    ) -> Result<tauri_plugin_updater::Updater, tauri_plugin_updater::Error> {
        let mut builder = app
            .updater_builder()
            .timeout(Duration::from_secs(30))
            .endpoints(endpoint_order(&channel.endpoints, attempt))?;
        if let Some(proxy) = crate::compat::ccswitch::network_proxy::updater_proxy_url() {
            builder = builder.proxy(proxy);
            log::info!("using configured local proxy for signed product update");
        }
        builder.build()
    }

    async fn wait_before_retry(
        &self,
        attempt: u8,
        stage: &str,
        error: &tauri_plugin_updater::Error,
    ) {
        let next_attempt = attempt + 1;
        log::warn!(
            "temporary signed update {stage} failure; retrying attempt {next_attempt}/{MAX_ATTEMPTS}: {}",
            update_error_detail(error)
        );
        self.set_status(|status| {
            status.phase = AppUpdatePhase::Checking;
            status.downloaded_bytes = 0;
            status.total_bytes = None;
            status.attempt = next_attempt;
        });
        tokio::time::sleep(backoff_after(attempt)).await;
    }

    fn fail(&self, technical: String) {
        log::warn!("{technical}");
        self.set_status(|status| status.phase = AppUpdatePhase::Failed);
    }

    fn set_status(&self, update: impl FnOnce(&mut AppUpdateStatus)) {
        let mut status = self
            .status
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        update(&mut status);
    }

    fn set_progress(&self, downloaded: u64, total: Option<u64>) {
        let mut status = self
            .status
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        status.downloaded_bytes = downloaded;
        status.total_bytes = total;
    }

    /// How long ago the updater last concluded that nothing newer exists.
    /// A backwards clock adjustment reports the answer as fully aged: one
    /// extra check costs nothing and the next conclusion resets the clock,
    /// so this cannot become a loop.
    fn up_to_date_age(&self) -> Option<Duration> {
        self.up_to_date_at
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .map(|at| at.elapsed().unwrap_or(RECHECK_AFTER))
    }

    fn mark_up_to_date(&self) {
        *self
            .up_to_date_at
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(SystemTime::now());
        self.set_status(|status| {
            status.phase = AppUpdatePhase::UpToDate;
            status.available_version = None;
        });
    }
}

/// Passive status consumers call `start(false)` while a background check is
/// active, so a phase that is already working must not be restarted. A
/// terminal failure must stay terminal until the user explicitly retries;
/// otherwise a restricted or offline network turns status polling into an
/// uncontrolled update-request loop. "Up to date" is different: it is a
/// statement about a moment, and once that moment is `RECHECK_AFTER` old the
/// application is allowed to ask again on its own.
fn should_start_check(
    phase: AppUpdatePhase,
    force: bool,
    up_to_date_age: Option<Duration>,
) -> bool {
    match phase {
        AppUpdatePhase::Idle => true,
        AppUpdatePhase::UpToDate => force || up_to_date_age.is_some_and(|age| age >= RECHECK_AFTER),
        AppUpdatePhase::Failed => force,
        AppUpdatePhase::Unconfigured
        | AppUpdatePhase::Checking
        | AppUpdatePhase::Downloading
        | AppUpdatePhase::Ready => false,
    }
}

fn empty_check_outcome(expected_version: Option<&str>) -> Result<(), &'static str> {
    if expected_version.is_some() {
        Err("signed update disappeared between automatic retries")
    } else {
        Ok(())
    }
}

fn update_error_detail(error: &impl std::fmt::Display) -> String {
    redact_secrets(&error.to_string())
        .chars()
        .take(1_000)
        .collect()
}

fn install_error(technical: String) -> AppError {
    AppError::new(ErrorCode::UpdateFailed, "error.appUpdate.installFailed")
        .with_technical(update_error_detail(&technical))
        .with_remediation("error.remediation.retryOrViewDetails")
}

#[cfg(test)]
mod tests {
    use super::RECHECK_AFTER;
    use crate::domain::AppUpdatePhase;
    use std::time::Duration;

    /// A conclusion so recent that no recheck is due.
    const FRESH: Option<Duration> = Some(Duration::from_secs(60));

    #[test]
    fn passive_polling_never_restarts_a_terminal_check() {
        assert!(super::should_start_check(AppUpdatePhase::Idle, false, None));
        assert!(!super::should_start_check(
            AppUpdatePhase::UpToDate,
            false,
            FRESH
        ));
        assert!(!super::should_start_check(
            AppUpdatePhase::Failed,
            false,
            None
        ));
        assert!(super::should_start_check(
            AppUpdatePhase::UpToDate,
            true,
            FRESH
        ));
        assert!(super::should_start_check(
            AppUpdatePhase::Failed,
            true,
            None
        ));

        for phase in [
            AppUpdatePhase::Unconfigured,
            AppUpdatePhase::Checking,
            AppUpdatePhase::Downloading,
            AppUpdatePhase::Ready,
        ] {
            assert!(!super::should_start_check(phase, false, None), "{phase:?}");
            assert!(!super::should_start_check(phase, true, None), "{phase:?}");
        }
    }

    #[test]
    fn an_aged_up_to_date_answer_lets_the_application_ask_again() {
        assert!(super::should_start_check(
            AppUpdatePhase::UpToDate,
            false,
            Some(RECHECK_AFTER)
        ));
        assert!(super::should_start_check(
            AppUpdatePhase::UpToDate,
            false,
            Some(RECHECK_AFTER + Duration::from_secs(1))
        ));
        assert!(!super::should_start_check(
            AppUpdatePhase::UpToDate,
            false,
            Some(RECHECK_AFTER - Duration::from_secs(1))
        ));
    }

    #[test]
    fn age_never_revives_a_failed_check() {
        // A restricted or offline network fails every attempt. If age could
        // restart it, passive status polling would become an unbounded retry
        // loop that the user never asked for.
        assert!(!super::should_start_check(
            AppUpdatePhase::Failed,
            false,
            Some(RECHECK_AFTER * 100)
        ));
    }

    #[test]
    fn a_retry_cannot_turn_a_discovered_update_into_up_to_date() {
        assert!(super::empty_check_outcome(None).is_ok());
        assert_eq!(
            super::empty_check_outcome(Some("1.2.4")),
            Err("signed update disappeared between automatic retries")
        );
    }
}
