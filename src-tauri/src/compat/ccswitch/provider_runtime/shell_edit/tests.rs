use std::path::PathBuf;

use super::{rewrite_line, split_keeping_endings, write_variable, ShellVariableEdit};

struct Home {
    root: tempfile::TempDir,
    backups: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("create a temporary home"),
            backups: tempfile::tempdir().expect("create a backup directory"),
        }
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.path().join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create directories");
        std::fs::write(&path, contents).expect("write start-up file");
        path
    }

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.root.path().join(relative)).expect("read back")
    }

    fn edit(
        &self,
        variable: &str,
        line: u32,
        expected: &str,
        new_value: &str,
    ) -> Result<PathBuf, crate::domain::AppError> {
        write_variable(
            self.root.path(),
            self.backups.path(),
            ShellVariableEdit {
                variable,
                line,
                expected_value: expected,
                new_value,
            },
        )
    }
}

#[test]
fn only_the_value_on_the_located_line_changes() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "# my profile\nexport EDITOR=vim\nexport ANTHROPIC_BASE_URL=https://old.example.test\nalias ll='ls -la'\n",
    );

    home.edit(
        "ANTHROPIC_BASE_URL",
        3,
        "https://old.example.test",
        "https://new.example.test",
    )
    .expect("rewrite the line");

    assert_eq!(
        home.read(".zshrc"),
        "# my profile\nexport EDITOR=vim\nexport ANTHROPIC_BASE_URL=https://new.example.test\nalias ll='ls -la'\n"
    );
}

/// A start-up file edited on Windows, or carried from there, uses CRLF. A
/// rewrite that normalised it would show every line as changed in the user's
/// own dotfile repository.
#[test]
fn line_endings_and_the_final_newline_survive() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "export EDITOR=vim\r\nexport ANTHROPIC_BASE_URL=https://old.example.test\r\nexport PAGER=less",
    );

    home.edit(
        "ANTHROPIC_BASE_URL",
        2,
        "https://old.example.test",
        "https://new.example.test",
    )
    .expect("rewrite the line");

    assert_eq!(
        home.read(".zshrc"),
        "export EDITOR=vim\r\nexport ANTHROPIC_BASE_URL=https://new.example.test\r\nexport PAGER=less",
        "CRLF and the missing final newline both stay as they were"
    );
}

#[test]
fn quoting_indentation_and_trailing_comments_are_preserved() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "  export ANTHROPIC_BASE_URL=\"https://old.example.test\" # work relay\n",
    );

    home.edit(
        "ANTHROPIC_BASE_URL",
        1,
        "https://old.example.test",
        "https://new.example.test",
    )
    .expect("rewrite the line");

    assert_eq!(
        home.read(".zshrc"),
        "  export ANTHROPIC_BASE_URL=\"https://new.example.test\" # work relay\n"
    );
}

#[test]
fn a_value_needing_quotes_gets_them() {
    let home = Home::new();
    home.write(".zshrc", "export SOME_NOTE=plain\n");

    home.edit("SOME_NOTE", 1, "plain", "two words")
        .expect("rewrite the line");

    assert_eq!(home.read(".zshrc"), "export SOME_NOTE='two words'\n");
}

/// The dialog's view can go stale while it is open. Writing anyway would edit
/// a line the user never read.
#[test]
fn a_line_that_changed_underneath_is_refused() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "export ANTHROPIC_BASE_URL=https://moved.example.test\n",
    );

    let error = home
        .edit(
            "ANTHROPIC_BASE_URL",
            1,
            "https://what-the-dialog-showed.example.test",
            "https://new.example.test",
        )
        .expect_err("refuse a stale edit");
    assert_eq!(error.message_key, "error.shellVariable.movedOn");
    assert_eq!(
        home.read(".zshrc"),
        "export ANTHROPIC_BASE_URL=https://moved.example.test\n",
        "a refused edit leaves the file untouched"
    );
}

#[test]
fn a_command_substitution_is_never_replaced_with_a_literal() {
    let home = Home::new();
    let original = "export ANTHROPIC_AUTH_TOKEN=\"$(security find-generic-password -w)\"\n";
    home.write(".zshrc", original);

    let error = home
        .edit(
            "ANTHROPIC_AUTH_TOKEN",
            1,
            "$(security find-generic-password -w)",
            "sk-typed-by-hand",
        )
        .expect_err("refuse to flatten a command substitution");
    assert_eq!(error.message_key, "error.shellVariable.notRewritable");
    assert_eq!(home.read(".zshrc"), original);
}

#[test]
fn a_replacement_that_would_split_the_line_is_refused() {
    let home = Home::new();
    let original = "export ANTHROPIC_BASE_URL=https://old.example.test\n";
    home.write(".zshrc", original);

    for attempt in ["https://x\nrm -rf /", "https://x\r\nwhoami"] {
        let error = home
            .edit("ANTHROPIC_BASE_URL", 1, "https://old.example.test", attempt)
            .expect_err("refuse a multi-line value");
        assert_eq!(error.message_key, "error.shellVariable.valueInvalid");
    }
    assert_eq!(home.read(".zshrc"), original);
}

#[test]
fn a_sourced_file_is_edited_in_place_rather_than_the_file_that_sourced_it() {
    let home = Home::new();
    home.write(".zshrc", "source ~/.config/zsh/secrets.zsh\n");
    home.write(
        ".config/zsh/secrets.zsh",
        "export ANTHROPIC_BASE_URL=https://old.example.test\n",
    );

    let edited = home
        .edit(
            "ANTHROPIC_BASE_URL",
            1,
            "https://old.example.test",
            "https://new.example.test",
        )
        .expect("rewrite the line");

    assert!(edited.ends_with("secrets.zsh"));
    assert_eq!(home.read(".zshrc"), "source ~/.config/zsh/secrets.zsh\n");
    assert_eq!(
        home.read(".config/zsh/secrets.zsh"),
        "export ANTHROPIC_BASE_URL=https://new.example.test\n"
    );
}

#[test]
fn the_original_is_copied_to_the_backup_directory_first() {
    let home = Home::new();
    let original = "export ANTHROPIC_BASE_URL=https://old.example.test\n";
    home.write(".zshrc", original);

    home.edit(
        "ANTHROPIC_BASE_URL",
        1,
        "https://old.example.test",
        "https://new.example.test",
    )
    .expect("rewrite the line");

    let backups: Vec<_> = std::fs::read_dir(home.backups.path())
        .expect("read the backup directory")
        .filter_map(Result::ok)
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        std::fs::read_to_string(backups[0].path()).expect("read the backup"),
        original,
        "the backup is the file itself, so a bad edit can be undone"
    );
}

#[cfg(unix)]
#[test]
fn the_file_keeps_its_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let home = Home::new();
    let path = home.write(
        ".zshrc",
        "export ANTHROPIC_BASE_URL=https://old.example.test\n",
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
        .expect("restrict the file");

    home.edit(
        "ANTHROPIC_BASE_URL",
        1,
        "https://old.example.test",
        "https://new.example.test",
    )
    .expect("rewrite the line");

    let mode = std::fs::metadata(&path)
        .expect("read metadata")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o777,
        0o600,
        "a secrets file must not become world-readable because we rewrote a line"
    );
}

#[test]
fn every_line_remembers_its_own_ending() {
    assert_eq!(
        split_keeping_endings("a\r\nb\nc"),
        vec![("a", "\r\n"), ("b", "\n"), ("c", "")]
    );
    assert_eq!(split_keeping_endings(""), Vec::<(&str, &str)>::new());
    assert_eq!(split_keeping_endings("\n"), vec![("", "\n")]);
}

#[test]
fn a_line_that_assigns_a_different_variable_is_not_rewritten() {
    assert_eq!(
        rewrite_line("export OTHER=1", "ANTHROPIC_BASE_URL", "x"),
        None
    );
    assert_eq!(rewrite_line("# not an assignment", "A", "x"), None);
}
