//! The product read model for backups (spec §92 / §18).
//!
//! **Deliberately free of any notion of "how the backup was made"** — no database file path, no
//! schema version, no safety-backup id. All the UI needs to say is "when it was made, how big it
//! is, what it is called"; everything else stays inside `compat/ccswitch/backup.rs` (the same
//! trick as keeping config payloads out of `Extension` in §36).

use serde::{Deserialize, Serialize};

/// What a backup looks like in the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    /// The real file name on disk, passed back verbatim on restore/delete; not first-screen list information.
    pub name: String,
    /// RFC 3339 creation time. Upstream takes the file modification time, and it is an empty string
    /// when unavailable — so the frontend must have a fallback and cannot assume it always parses.
    pub created_at: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupList {
    /// Newest first (the upstream `list_backups` already sorts by creation time descending).
    ///
    /// Deliberately without the path of the backup folder: that is an absolute path containing the
    /// user name, and paths do not cross IPC. The UI only needs to say "backups are kept in this
    /// computer's application data directory".
    pub files: Vec<BackupFile>,
}

/// The result of a restore. **"The restore failed" and "the restore succeeded but the write-back
/// was incomplete" are two different things** (decision 8): the former takes `Err`, the latter this
/// boolean — the database really has been replaced, and reporting that as a failure would tempt the
/// user into restoring again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    pub backups: BackupList,
    /// The database restore finished, but one step of writing "which service is in use" back into the tool configs did not succeed.
    pub tools_out_of_sync: bool,
}

/// Result of the native save dialog and archive export. The selected path is
/// intentionally absent from the wire format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum BackupExportOutcome {
    Cancelled,
    Exported,
}

/// Result of the native open dialog and archive import. Import replaces the
/// managed database, so it returns the same authoritative inventory and live
/// projection warning as an ordinary restore without exposing the source path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "status",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum BackupImportOutcome {
    Cancelled,
    Imported {
        backups: BackupList,
        tools_out_of_sync: bool,
    },
}

/// The upstream interval that means "once a day". The product never offers
/// any other cadence, so the wire only carries on/off.
pub const DAILY_BACKUP_INTERVAL_HOURS: u32 = 24;

/// The automatic backup policy in product terms: on or off, and how many
/// restore points to keep. Upstream stores an interval in hours and a retain
/// count; both defaults and the "0 = off" convention live behind this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupSchedule {
    pub automatic: bool,
    pub retain_count: u32,
}

impl BackupSchedule {
    pub fn from_upstream(interval_hours: u32, retain_count: u32) -> Self {
        Self {
            automatic: interval_hours > 0,
            retain_count: retain_count.max(1),
        }
    }

    /// Upstream never accepts a retain count of zero; it would delete every
    /// restore point as soon as the next one is written.
    pub fn normalized(self) -> Self {
        Self {
            retain_count: self.retain_count.max(1),
            ..self
        }
    }

    pub fn interval_hours(self) -> u32 {
        if self.automatic {
            DAILY_BACKUP_INTERVAL_HOURS
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackupExportOutcome, BackupFile, BackupImportOutcome, BackupList, BackupSchedule,
        RestoreOutcome,
    };

    fn file() -> BackupFile {
        BackupFile {
            name: "db_backup_20260819_101500.db".to_string(),
            created_at: "2026-08-19T10:15:00+08:00".to_string(),
            size_bytes: 2_097_152,
        }
    }

    #[test]
    fn the_wire_format_is_camel_case() {
        let json = serde_json::to_string(&file()).expect("serialize backup file");
        assert_eq!(
            json,
            r#"{"name":"db_backup_20260819_101500.db","createdAt":"2026-08-19T10:15:00+08:00","sizeBytes":2097152}"#
        );
    }

    #[test]
    fn a_restore_reports_the_refreshed_list_and_whether_the_tools_kept_up() {
        let outcome = RestoreOutcome {
            backups: BackupList {
                files: vec![file()],
            },
            tools_out_of_sync: true,
        };
        let json = serde_json::to_string(&outcome).expect("serialize outcome");
        assert!(json.contains(r#""toolsOutOfSync":true"#));
        assert!(json.contains(r#""files":[{"name":"db_backup_20260819_101500.db""#));
        let parsed: RestoreOutcome = serde_json::from_str(&json).expect("deserialize outcome");
        assert_eq!(parsed, outcome);
    }

    #[test]
    fn the_model_carries_nothing_about_how_a_backup_is_made() {
        // §42 / §99: words like "database", "schema" and "safety backup id" must not appear in the
        // UI. Keeping them out of the wire format is more reliable than asking the frontend to
        // "be careful not to display them".
        let json = serde_json::to_string(&BackupList {
            files: vec![file()],
        })
        .expect("serialize list");
        for leaked in [
            "schema",
            "database",
            "safety",
            "sql",
            "version",
            "directory",
            "/",
        ] {
            assert!(!json.contains(leaked), "{leaked} must not be on the wire");
        }
    }

    #[test]
    fn archive_outcomes_never_expose_the_selected_archive_path() {
        assert_eq!(
            serde_json::to_string(&BackupExportOutcome::Cancelled).expect("cancelled export"),
            r#"{"status":"cancelled"}"#
        );
        assert_eq!(
            serde_json::to_string(&BackupExportOutcome::Exported).expect("completed export"),
            r#"{"status":"exported"}"#
        );

        let imported = serde_json::to_string(&BackupImportOutcome::Imported {
            backups: BackupList {
                files: vec![file()],
            },
            tools_out_of_sync: false,
        })
        .expect("completed import");
        assert!(imported.contains(r#""status":"imported""#));
        assert!(imported.contains(r#""toolsOutOfSync":false"#));
        for forbidden in ["sourcePath", "selectedPath", "archivePath", ".sql"] {
            assert!(!imported.contains(forbidden), "wire leaked {forbidden}");
        }
    }

    #[test]
    fn the_schedule_maps_the_upstream_interval_to_a_daily_switch() {
        assert_eq!(
            BackupSchedule::from_upstream(24, 10),
            BackupSchedule {
                automatic: true,
                retain_count: 10,
            }
        );
        assert!(!BackupSchedule::from_upstream(0, 10).automatic);
        assert_eq!(BackupSchedule::from_upstream(24, 0).retain_count, 1);

        let on = BackupSchedule {
            automatic: true,
            retain_count: 0,
        };
        assert_eq!(on.interval_hours(), 24);
        assert_eq!(on.normalized().retain_count, 1);
        assert_eq!(
            BackupSchedule {
                automatic: false,
                retain_count: 5,
            }
            .interval_hours(),
            0
        );
    }

    #[test]
    fn the_schedule_wire_is_camel_case_and_strict() {
        let schedule = BackupSchedule {
            automatic: true,
            retain_count: 10,
        };
        assert_eq!(
            serde_json::to_string(&schedule).expect("serialize schedule"),
            r#"{"automatic":true,"retainCount":10}"#
        );
        assert!(serde_json::from_str::<BackupSchedule>(
            r#"{"automatic":true,"retainCount":10,"intervalHours":24}"#
        )
        .is_err());
    }
}
