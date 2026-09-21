use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Stable error codes (spec §43). The literals go into frontend branching and logs, so renaming one counts as a breaking change.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    ToolNotFound,
    InstallFailed,
    UpdateFailed,
    UpdatePreviewStale,
    UninstallFailed,
    LaunchFailed,
    ConfigParseFailed,
    ConfigWriteFailed,
    ProviderUnreachable,
    ProviderNotFound,
    SessionNotFound,
    ExtensionNotFound,
    BackupNotFound,
    McpUnavailable,
    PermissionDenied,
    NetworkError,
    OperationConflict,
    UpstreamError,
    Internal,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ToolNotFound => "TOOL_NOT_FOUND",
            Self::InstallFailed => "INSTALL_FAILED",
            Self::UpdateFailed => "UPDATE_FAILED",
            Self::UpdatePreviewStale => "UPDATE_PREVIEW_STALE",
            Self::UninstallFailed => "UNINSTALL_FAILED",
            Self::LaunchFailed => "LAUNCH_FAILED",
            Self::ConfigParseFailed => "CONFIG_PARSE_FAILED",
            Self::ConfigWriteFailed => "CONFIG_WRITE_FAILED",
            Self::ProviderUnreachable => "PROVIDER_UNREACHABLE",
            Self::ProviderNotFound => "PROVIDER_NOT_FOUND",
            Self::SessionNotFound => "SESSION_NOT_FOUND",
            Self::ExtensionNotFound => "EXTENSION_NOT_FOUND",
            Self::BackupNotFound => "BACKUP_NOT_FOUND",
            Self::McpUnavailable => "MCP_UNAVAILABLE",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::NetworkError => "NETWORK_ERROR",
            Self::OperationConflict => "OPERATION_CONFLICT",
            Self::UpstreamError => "UPSTREAM_ERROR",
            Self::Internal => "INTERNAL",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The product error model (spec §42). A separate model from the upstream
/// `crate::error::AppError`: upstream serializes to a single localized string, while this type
/// serializes to structured JSON and the frontend branches only on `code`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Error)]
#[serde(rename_all = "camelCase")]
#[error("[{code}] {message_key}")]
pub struct AppError {
    pub code: ErrorCode,
    pub message_key: String,
    pub technical_message: Option<String>,
    pub remediation: Option<String>,
    pub context_id: Option<String>,
}

impl AppError {
    pub fn new(code: ErrorCode, message_key: impl Into<String>) -> Self {
        Self {
            code,
            message_key: message_key.into(),
            technical_message: None,
            remediation: None,
            context_id: None,
        }
    }

    pub fn with_technical(mut self, technical: impl Into<String>) -> Self {
        self.technical_message = Some(technical.into());
        self
    }

    pub fn with_remediation(mut self, remediation: impl Into<String>) -> Self {
        self.remediation = Some(remediation.into());
        self
    }

    pub fn with_context_id(mut self, context_id: impl Into<String>) -> Self {
        self.context_id = Some(context_id.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{AppError, ErrorCode};

    #[test]
    fn error_code_strings_are_stable() {
        let pairs = [
            (ErrorCode::ToolNotFound, "TOOL_NOT_FOUND"),
            (ErrorCode::InstallFailed, "INSTALL_FAILED"),
            (ErrorCode::UpdateFailed, "UPDATE_FAILED"),
            (ErrorCode::UpdatePreviewStale, "UPDATE_PREVIEW_STALE"),
            (ErrorCode::UninstallFailed, "UNINSTALL_FAILED"),
            (ErrorCode::LaunchFailed, "LAUNCH_FAILED"),
            (ErrorCode::ConfigParseFailed, "CONFIG_PARSE_FAILED"),
            (ErrorCode::ConfigWriteFailed, "CONFIG_WRITE_FAILED"),
            (ErrorCode::ProviderUnreachable, "PROVIDER_UNREACHABLE"),
            (ErrorCode::ProviderNotFound, "PROVIDER_NOT_FOUND"),
            (ErrorCode::SessionNotFound, "SESSION_NOT_FOUND"),
            (ErrorCode::ExtensionNotFound, "EXTENSION_NOT_FOUND"),
            (ErrorCode::BackupNotFound, "BACKUP_NOT_FOUND"),
            (ErrorCode::McpUnavailable, "MCP_UNAVAILABLE"),
            (ErrorCode::PermissionDenied, "PERMISSION_DENIED"),
            (ErrorCode::NetworkError, "NETWORK_ERROR"),
            (ErrorCode::OperationConflict, "OPERATION_CONFLICT"),
            (ErrorCode::UpstreamError, "UPSTREAM_ERROR"),
            (ErrorCode::Internal, "INTERNAL"),
        ];
        for (code, expected) in pairs {
            assert_eq!(code.as_str(), expected);
            assert_eq!(code.to_string(), expected);
            let json = serde_json::to_string(&code).expect("serialize error code");
            assert_eq!(json, format!("\"{expected}\""));
        }
    }

    #[test]
    fn full_error_round_trips_through_json() {
        let minimal = AppError::new(ErrorCode::ToolNotFound, "error.tool.notFound");
        assert_eq!(
            serde_json::to_string(&minimal).expect("serialize app error"),
            r#"{"code":"TOOL_NOT_FOUND","messageKey":"error.tool.notFound","technicalMessage":null,"remediation":null,"contextId":null}"#
        );

        let error = AppError::new(ErrorCode::InstallFailed, "error.tool.installFailed")
            .with_technical("exit code 127")
            .with_remediation("error.remediation.retryInstall")
            .with_context_id("op-42");
        let json = serde_json::to_string(&error).expect("serialize app error");
        assert_eq!(
            json,
            r#"{"code":"INSTALL_FAILED","messageKey":"error.tool.installFailed","technicalMessage":"exit code 127","remediation":"error.remediation.retryInstall","contextId":"op-42"}"#
        );
        let parsed: AppError = serde_json::from_str(&json).expect("deserialize app error");
        assert_eq!(parsed, error);
    }

    #[test]
    fn display_is_code_prefixed_and_hides_technical_detail() {
        let error = AppError::new(ErrorCode::NetworkError, "error.network.unreachable")
            .with_technical("connection refused to 127.0.0.1:1234");
        assert_eq!(
            error.to_string(),
            "[NETWORK_ERROR] error.network.unreachable"
        );
    }
}
