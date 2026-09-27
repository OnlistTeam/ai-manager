//! Ending a route: put back only the route's own settings (ADR-0054).
//!
//! Turning one tool's routing off restores its config through the inherited
//! restore, fed a backup narrowed to the settings the route wrote, and leaves
//! the local gateway running: a session of that tool that is already open
//! may still hold the gateway's address and keeps working until it restarts.
//! The gateway stops when AI Manager quits or the user stops everything.

use serde_json::Value;

use crate::app_config::AppType;
use crate::domain::{AppError, RoutedTool};

use super::owned_settings::{owned_settings, restore_own_settings, OwnedSetting};
use super::{change_failed, RoutingApp, RoutingStore, ROUTING_APPS};

const OWNED_KEY_PREFIX: &str = "aimgr.routing.owned.";
/// What the inherited takeover writes in place of a real key.
const ROUTE_TOKEN_PLACEHOLDER: &str = "PROXY_MANAGED";

fn owned_key(app: &str) -> String {
    format!("{OWNED_KEY_PREFIX}{app}")
}

impl RoutingStore {
    async fn backup_config(&self, app: &str) -> Option<Value> {
        let backup = self.db.get_live_backup(app).await.ok()??;
        serde_json::from_str(&backup.original_config).ok()
    }

    /// Notes which settings the route wrote for `entry`, after every change
    /// that rewrites the routed config. Without a note the route's end
    /// restores the whole backup, as the inherited path does.
    pub(super) async fn note_owned_settings(&self, entry: &RoutingApp) {
        let Some(backup) = self.backup_config(entry.app).await else {
            self.forget_owned(entry.app);
            return;
        };
        let routed = match (entry.read_live)() {
            Ok(routed) => routed,
            Err(error) => {
                log::warn!("Could not read the routed {} config: {error}", entry.app);
                self.forget_owned(entry.app);
                return;
            }
        };
        let owned = owned_settings(&backup, &routed, entry.layout);
        let saved = serde_json::to_string(&owned)
            .map_err(|error| error.to_string())
            .and_then(|text| {
                self.db
                    .set_setting(&owned_key(entry.app), &text)
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = saved {
            log::warn!("Could not note the {} route's settings: {error}", entry.app);
        }
    }

    fn load_owned(&self, app: &str) -> Option<Vec<OwnedSetting>> {
        let text = self.db.get_setting(&owned_key(app)).ok()??;
        if text.is_empty() {
            return None;
        }
        serde_json::from_str(&text).ok()
    }

    fn forget_owned(&self, app: &str) {
        if let Err(error) = self.db.set_setting(&owned_key(app), "") {
            log::warn!("Could not clear the {app} route's settings note: {error}");
        }
    }

    /// How the local route appears inside a routed config.
    async fn route_markers(&self) -> Vec<String> {
        let mut markers = vec![ROUTE_TOKEN_PLACEHOLDER.to_string()];
        let port = match self.proxy.get_status().await {
            Ok(status) if status.running => status.port,
            _ => self
                .db
                .get_proxy_config()
                .await
                .map(|config| config.listen_port)
                .unwrap_or(0),
        };
        if port != 0 {
            for host in ["127.0.0.1", "localhost", "[::1]", "0.0.0.0"] {
                markers.push(format!("{host}:{port}"));
            }
        }
        markers
    }

    /// Replaces the backup with what the route's end should leave: the
    /// current file with only the route's own settings put back.
    async fn narrow_backup(&self, entry: &RoutingApp, markers: &[String]) {
        let Some(owned) = self.load_owned(entry.app) else {
            return;
        };
        let Some(backup) = self.backup_config(entry.app).await else {
            return;
        };
        let Ok(current) = (entry.read_live)() else {
            return;
        };
        let narrowed = restore_own_settings(&backup, &current, &owned, entry.layout, markers);
        let saved = match serde_json::to_string(&narrowed) {
            Ok(text) => self
                .db
                .save_live_backup(entry.app, &text)
                .await
                .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = saved {
            log::warn!(
                "Could not narrow the {} backup; the whole backup is restored: {error}",
                entry.app
            );
        }
    }

    /// Ends the route for one tool. Callers hold `mutation_lock`.
    pub(super) async fn release_unlocked(&self, entry: &RoutingApp) -> Result<(), AppError> {
        // Upstream keeps the failover switch across takeover-off. Clear it
        // first: "takeover off, failover on" would let the next takeover
        // revive failover without the P1 switch `set_failover` requires.
        self.clear_auto_failover(entry.app).await?;
        let mut config = self
            .db
            .get_proxy_config_for_app(entry.app)
            .await
            .map_err(change_failed)?;
        if !config.enabled {
            self.forget_owned(entry.app);
            return Ok(());
        }
        let markers = self.route_markers().await;
        self.narrow_backup(entry, &markers).await;
        let app_type: AppType = entry.app.parse().map_err(change_failed)?;
        {
            let _switch = self.proxy.lock_switch_for_app(entry.app).await;
            self.proxy
                .restore_live_config_for_app_with_fallback_inner(&app_type)
                .await
                .map_err(change_failed)?;
        }
        // The inherited takeover-off does the same bookkeeping and then stops
        // the gateway once no tool is routed; the product keeps it running.
        self.db
            .delete_live_backup(entry.app)
            .await
            .map_err(change_failed)?;
        config.enabled = false;
        self.db
            .update_proxy_config_for_app(config)
            .await
            .map_err(change_failed)?;
        self.db
            .clear_provider_health_for_app(entry.app)
            .await
            .map_err(change_failed)?;
        self.forget_owned(entry.app);
        Ok(())
    }

    /// Stops the gateway and puts every routed tool back. Callers hold
    /// `mutation_lock`.
    pub(super) async fn stop_all_unlocked(&self) -> Result<(), AppError> {
        // Upstream deliberately keeps every failover switch across a stop; the
        // product clears them for the same reason as a single tool's release.
        for entry in &ROUTING_APPS {
            self.clear_auto_failover(entry.app).await?;
        }
        let markers = self.route_markers().await;
        for entry in &ROUTING_APPS {
            self.narrow_backup(entry, &markers).await;
        }
        self.proxy
            .stop_with_restore()
            .await
            .map_err(change_failed)?;
        for entry in &ROUTING_APPS {
            self.forget_owned(entry.app);
        }
        Ok(())
    }

    /// The tools routed through AI Manager right now.
    pub async fn routed_tools(&self) -> Result<Vec<RoutedTool>, AppError> {
        let mut routed = Vec::new();
        for entry in &ROUTING_APPS {
            let config = self
                .db
                .get_proxy_config_for_app(entry.app)
                .await
                .map_err(change_failed)?;
            if config.enabled {
                routed.push(RoutedTool {
                    tool: entry.tool,
                    pickup: entry.pickup,
                });
            }
        }
        Ok(routed)
    }

    async fn anything_to_put_back(&self) -> bool {
        let routed = self
            .routed_tools()
            .await
            .is_ok_and(|tools| !tools.is_empty());
        let backups = self.db.has_any_live_backup().await.unwrap_or(false);
        routed || backups || self.proxy.detect_takeover_in_live_configs()
    }

    /// At launch every tool starts direct: a route an earlier run left behind
    /// (a crash, an update restart) is put back and its switch cleared.
    pub async fn recover_at_launch(&self) {
        let _guard = self.mutation_lock.lock().await;
        let leftovers = self.db.has_any_live_backup().await.unwrap_or(false)
            || self.proxy.detect_takeover_in_live_configs();
        if leftovers {
            let markers = self.route_markers().await;
            for entry in &ROUTING_APPS {
                self.narrow_backup(entry, &markers).await;
            }
            if let Err(error) = self.proxy.recover_from_crash().await {
                log::error!("Could not put back the tools left routed: {error}");
            }
        }
        for entry in &ROUTING_APPS {
            let _ = self.clear_auto_failover(entry.app).await;
            if let Ok(mut config) = self.db.get_proxy_config_for_app(entry.app).await {
                if config.enabled {
                    config.enabled = false;
                    if let Err(error) = self.db.update_proxy_config_for_app(config).await {
                        log::warn!("Could not clear the {} route switch: {error}", entry.app);
                    }
                }
            }
            self.forget_owned(entry.app);
        }
    }

    /// Before AI Manager quits: stop the gateway and put every routed tool
    /// back, so no tool is left pointing at an address nothing answers.
    pub async fn release_before_exit(&self) {
        let _guard = self.mutation_lock.lock().await;
        if self.anything_to_put_back().await {
            if let Err(error) = self.stop_all_unlocked().await {
                log::error!("Could not put the routed tools back before quitting: {error:?}");
            }
        } else if self.proxy.is_running().await {
            if let Err(error) = self.proxy.stop().await {
                log::error!("Could not stop the local gateway before quitting: {error}");
            }
        }
    }
}

#[cfg(test)]
#[path = "tests_release.rs"]
mod tests;
