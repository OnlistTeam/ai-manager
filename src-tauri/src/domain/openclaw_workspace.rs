use serde::{Deserialize, Serialize};

/// Product-owned identifiers for OpenClaw's documented workspace files.
/// The renderer can select one of these values but can never provide a path.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum OpenClawWorkspaceFileId {
    Agents,
    Soul,
    User,
    Identity,
    Tools,
    Memory,
    Heartbeat,
    Bootstrap,
    Boot,
}

impl OpenClawWorkspaceFileId {
    pub const ALL: [Self; 9] = [
        Self::Agents,
        Self::Soul,
        Self::User,
        Self::Identity,
        Self::Tools,
        Self::Memory,
        Self::Heartbeat,
        Self::Bootstrap,
        Self::Boot,
    ];

    pub const fn filename(self) -> &'static str {
        match self {
            Self::Agents => "AGENTS.md",
            Self::Soul => "SOUL.md",
            Self::User => "USER.md",
            Self::Identity => "IDENTITY.md",
            Self::Tools => "TOOLS.md",
            Self::Memory => "MEMORY.md",
            Self::Heartbeat => "HEARTBEAT.md",
            Self::Bootstrap => "BOOTSTRAP.md",
            Self::Boot => "BOOT.md",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OpenClawWorkspaceFileStatus {
    Missing,
    Ready,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawWorkspaceFileSummary {
    pub id: OpenClawWorkspaceFileId,
    pub filename: String,
    pub status: OpenClawWorkspaceFileStatus,
    pub size_bytes: u64,
    pub modified_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawWorkspaceOverview {
    pub files: Vec<OpenClawWorkspaceFileSummary>,
    pub existing_files: u32,
    pub daily_memory_count: u32,
    pub daily_memory_bytes: u64,
    pub total_bytes: u64,
    pub limited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawWorkspaceDocument {
    pub id: OpenClawWorkspaceFileId,
    pub filename: String,
    pub exists: bool,
    pub content: String,
    pub size_bytes: u64,
    pub modified_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawDailyMemorySummary {
    pub date: String,
    pub size_bytes: u64,
    pub modified_at: Option<u64>,
    pub preview: String,
    pub match_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawDailyMemoryList {
    pub items: Vec<OpenClawDailyMemorySummary>,
    pub total_count: u32,
    pub total_bytes: u64,
    pub limited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawDailyMemoryDocument {
    pub date: String,
    pub exists: bool,
    pub content: String,
    pub size_bytes: u64,
    pub modified_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OpenClawWorkspaceWriteOutcome {
    pub backup_created: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum OpenClawWorkspaceDirectory {
    Workspace,
    DailyMemory,
}

#[cfg(test)]
mod tests {
    use super::{
        OpenClawDailyMemoryDocument, OpenClawWorkspaceDocument, OpenClawWorkspaceFileId,
        OpenClawWorkspaceOverview,
    };

    #[test]
    fn file_ids_have_a_fixed_path_free_wire_format() {
        let encoded = serde_json::to_string(&OpenClawWorkspaceFileId::Heartbeat)
            .expect("serialize workspace file id");
        assert_eq!(encoded, "\"heartbeat\"");
        assert_eq!(
            OpenClawWorkspaceFileId::Heartbeat.filename(),
            "HEARTBEAT.md"
        );
        assert_eq!(OpenClawWorkspaceFileId::ALL.len(), 9);
    }

    #[test]
    fn workspace_payloads_never_contain_a_raw_path_field() {
        let document = OpenClawWorkspaceDocument {
            id: OpenClawWorkspaceFileId::Memory,
            filename: "MEMORY.md".to_string(),
            exists: true,
            content: "Remember this".to_string(),
            size_bytes: 13,
            modified_at: Some(42),
        };
        let memory = OpenClawDailyMemoryDocument {
            date: "2026-08-26".to_string(),
            exists: true,
            content: "Daily note".to_string(),
            size_bytes: 10,
            modified_at: Some(43),
        };
        let encoded = serde_json::to_string(&(document, memory)).expect("serialize payloads");
        for forbidden in ["/Users/", "\\Users\\", "path", "directory", ".openclaw"] {
            assert!(!encoded.contains(forbidden), "payload leaked {forbidden}");
        }

        let overview = OpenClawWorkspaceOverview {
            files: Vec::new(),
            existing_files: 0,
            daily_memory_count: 0,
            daily_memory_bytes: 0,
            total_bytes: 0,
            limited: false,
        };
        assert_eq!(
            serde_json::to_value(overview).expect("serialize overview")["dailyMemoryBytes"],
            0
        );
    }
}
