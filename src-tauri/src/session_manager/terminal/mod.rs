/// POSIX single-quote escaping.
///
/// **Single quotes are mandatory**: inside double quotes `$(...)`, backticks and `$VAR`
/// still expand, and what we wrap here is `projectDir` — the real project path recorded
/// in session history. macOS allows `$`, `(` and `)` in directory names, so a directory
/// named `$(...)` is enough to make command substitution run in the user's terminal.
///
/// Nothing expands inside single quotes; the only character that cannot be represented
/// is `'` itself, worked around with the close-escape-reopen sequence `'\''`.
pub(crate) fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_escape_neutralizes_command_substitution_in_directory_names() {
        // All of these characters are legal in macOS directory names, and `cwd` is the
        // `projectDir` from session history: a directory named `$(...)` must land after
        // `cd` verbatim, never evaluated by the shell. The old double-quoted implementation
        // failed on all three.
        assert_eq!(shell_escape("/tmp/$(id -un)"), "'/tmp/$(id -un)'");
        assert_eq!(shell_escape("/tmp/`id -un`"), "'/tmp/`id -un`'");
        assert_eq!(shell_escape("/tmp/$HOME"), "'/tmp/$HOME'");
    }

    #[test]
    fn shell_escape_handles_embedded_single_quote() {
        // A single quote is the one character single-quoting cannot represent; handled by
        // the close-escape-reopen trick.
        assert_eq!(shell_escape("/tmp/it's"), r"'/tmp/it'\''s'");
    }
}
