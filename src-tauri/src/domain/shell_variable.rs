//! Editing a connection variable a shell profile sets (ADR-0042).

use serde::{Deserialize, Serialize};

/// Where a connection variable is written down, and whether this product can
/// safely change it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellVariableLocation {
    pub variable: String,
    /// The start-up file, shortened to `~/…` for display. Never an absolute
    /// path outside the home directory in the common case, and never used by
    /// the renderer to address a write: the write re-locates the line itself.
    pub path: String,
    /// 1-based, so it reads the same as it does in an editor.
    pub line: u32,
    /// The value on that line, so the dialog can prefill it and so the write
    /// can refuse if the line has changed since.
    pub value: String,
    /// False when the line is a command substitution or an interpolation,
    /// where replacing the text with a literal would change what the line
    /// does rather than what it says. The edit action is withheld.
    pub editable: bool,
}

/// Redacted: the value is a credential as often as it is an address.
impl std::fmt::Debug for ShellVariableLocation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellVariableLocation")
            .field("variable", &self.variable)
            .field("path", &"<path>")
            .field("line", &self.line)
            .field("value", &"<redacted>")
            .field("editable", &self.editable)
            .finish()
    }
}

/// One replacement, carrying what the caller last saw so a line that moved on
/// cannot be overwritten blindly.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellVariableUpdate {
    pub variable: String,
    pub line: u32,
    pub expected_value: String,
    pub new_value: String,
}

impl std::fmt::Debug for ShellVariableUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellVariableUpdate")
            .field("variable", &self.variable)
            .field("line", &self.line)
            .field("expected_value", &"<redacted>")
            .field("new_value", &"<redacted>")
            .finish()
    }
}

/// What changed, for the dialog to report without guessing.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellVariableWritten {
    /// The file that was rewritten, shortened for display.
    pub path: String,
    pub line: u32,
}

impl std::fmt::Debug for ShellVariableWritten {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShellVariableWritten")
            .field("path", &"<path>")
            .field("line", &self.line)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_location_never_renders_its_value() {
        let location = ShellVariableLocation {
            variable: "ANTHROPIC_AUTH_TOKEN".to_string(),
            path: "~/.config/zsh/secrets.zsh".to_string(),
            line: 14,
            value: "sk-must-not-be-logged".to_string(),
            editable: true,
        };
        let rendered = format!("{location:?}");
        assert!(!rendered.contains("sk-must-not-be-logged"), "{rendered}");
        assert!(!rendered.contains("secrets.zsh"), "{rendered}");
        // The variable name and the line number are safe and are what a log
        // needs in order to be useful at all.
        assert!(rendered.contains("ANTHROPIC_AUTH_TOKEN"), "{rendered}");
        assert!(rendered.contains("14"), "{rendered}");
    }

    #[test]
    fn an_update_never_renders_either_value() {
        let update = ShellVariableUpdate {
            variable: "ANTHROPIC_BASE_URL".to_string(),
            line: 3,
            expected_value: "https://old.example.test".to_string(),
            new_value: "https://new.example.test".to_string(),
        };
        let rendered = format!("{update:?}");
        assert!(!rendered.contains("example.test"), "{rendered}");
    }
}
