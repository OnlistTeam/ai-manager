use std::path::PathBuf;

use super::{
    locate_variable, parse_assignment, resolve_sourced, responsible_site, sourced_arguments,
    unquote,
};

/// A throwaway home directory, so every test builds a real start-up tree on
/// disk rather than a fake one: the whole point of this module is what happens
/// when `source` sends it somewhere else.
struct Home {
    root: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            root: tempfile::tempdir().expect("create a temporary home"),
        }
    }

    fn path(&self) -> &std::path::Path {
        self.root.path()
    }

    fn write(&self, relative: &str, contents: &str) -> PathBuf {
        let path = self.root.path().join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("create directories");
        std::fs::write(&path, contents).expect("write start-up file");
        path
    }
}

#[test]
fn an_export_in_a_scanned_file_is_located_with_its_line() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "# a comment\nexport EDITOR=vim\nexport ANTHROPIC_BASE_URL=https://relay.example.test\n",
    );

    let sites = locate_variable(home.path(), "ANTHROPIC_BASE_URL");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].line, 3);
    assert_eq!(sites[0].value, "https://relay.example.test");
    assert!(sites[0].rewritable);
}

/// The case that made this module necessary: a small `.zshrc` that sources a
/// secrets file, with the variable in the sourced file. The upstream scanner
/// reports only "something in your shell sets this".
#[test]
fn a_variable_behind_a_source_directive_is_found() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "export EDITOR=vim\nsource \"$HOME/.config/zsh/secrets.zsh\"\n",
    );
    home.write(
        ".config/zsh/secrets.zsh",
        "export ANTHROPIC_AUTH_TOKEN=sk-from-the-sourced-file\n",
    );

    let sites = locate_variable(home.path(), "ANTHROPIC_AUTH_TOKEN");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].line, 1);
    assert_eq!(sites[0].value, "sk-from-the-sourced-file");
    assert!(sites[0].path.ends_with("secrets.zsh"));
}

#[test]
fn the_source_argument_is_expanded_in_the_forms_a_profile_uses() {
    let home = Home::new();
    for (index, directive) in [
        "source ~/.config/zsh/one.zsh",
        ". \"${HOME}/.config/zsh/two.zsh\"",
        "source $XDG_CONFIG_HOME/zsh/three.zsh",
    ]
    .iter()
    .enumerate()
    {
        home.write(
            &format!(".config/zsh/{}.zsh", ["one", "two", "three"][index]),
            &format!("export ANTHROPIC_BASE_URL=https://{}.example.test\n", index),
        );
        home.write(".zshrc", directive);

        let sites = locate_variable(home.path(), "ANTHROPIC_BASE_URL");
        assert_eq!(sites.len(), 1, "{directive}");
        assert_eq!(sites[0].value, format!("https://{index}.example.test"));
    }
}

/// The idiom everybody uses for an optional file, and the reason this module
/// missed the very setup it was written for until it was fixed.
#[test]
fn a_guarded_source_directive_is_followed() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "[[ -r ~/.config/zsh/secrets.zsh ]] && source ~/.config/zsh/secrets.zsh\n",
    );
    home.write(
        ".config/zsh/secrets.zsh",
        "# credentials\nexport ANTHROPIC_BASE_URL=https://relay.example.test\n",
    );

    let sites = locate_variable(home.path(), "ANTHROPIC_BASE_URL");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].line, 2);
    assert!(sites[0].path.ends_with("secrets.zsh"));
}

#[test]
fn directives_are_found_wherever_they_sit_on_the_line() {
    assert_eq!(
        sourced_arguments("[[ -r ~/a.zsh ]] && source ~/a.zsh"),
        vec!["~/a.zsh"]
    );
    assert_eq!(
        sourced_arguments("[ -f ~/a.zsh ] && . ~/a.zsh"),
        vec!["~/a.zsh"]
    );
    assert_eq!(
        sourced_arguments("source ~/a.zsh; source ~/b.zsh"),
        vec!["~/a.zsh", "~/b.zsh"]
    );
    assert_eq!(sourced_arguments("# source ~/a.zsh"), Vec::<&str>::new());
    assert_eq!(sourced_arguments("resource ~/a.zsh"), Vec::<&str>::new());
}

#[test]
fn a_default_valued_parameter_in_a_source_path_is_expanded() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "source \"${XDG_CONFIG_HOME:-$HOME/.config}/zsh/secrets.zsh\"\n",
    );
    home.write(
        ".config/zsh/secrets.zsh",
        "export ANTHROPIC_BASE_URL=https://relay.example.test\n",
    );
    assert_eq!(locate_variable(home.path(), "ANTHROPIC_BASE_URL").len(), 1);
}

#[test]
fn a_commented_out_export_is_not_a_site() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "#export ANTHROPIC_BASE_URL=https://old.example.test\n# export ANTHROPIC_BASE_URL=https://older.example.test\n",
    );
    assert!(locate_variable(home.path(), "ANTHROPIC_BASE_URL").is_empty());
}

/// A cycle is not exotic — two files that source each other is a common typo —
/// and it must not spin.
#[test]
fn a_cycle_of_source_directives_terminates() {
    let home = Home::new();
    home.write(".zshrc", "source ~/.config/zsh/a.zsh\n");
    home.write(
        ".config/zsh/a.zsh",
        "source ~/.config/zsh/b.zsh\nexport ANTHROPIC_BASE_URL=https://a.example.test\n",
    );
    home.write(".config/zsh/b.zsh", "source ~/.config/zsh/a.zsh\n");

    let sites = locate_variable(home.path(), "ANTHROPIC_BASE_URL");
    assert_eq!(sites.len(), 1);
}

#[test]
fn a_source_of_a_file_that_is_not_here_is_not_an_error() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "source ~/.config/zsh/only-on-the-work-laptop.zsh\nexport ANTHROPIC_BASE_URL=https://relay.example.test\n",
    );
    let sites = locate_variable(home.path(), "ANTHROPIC_BASE_URL");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].line, 2);
}

#[test]
fn an_interpolated_value_is_located_but_not_rewritable() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "export ANTHROPIC_AUTH_TOKEN=\"$(security find-generic-password -w -s anthropic)\"\n",
    );
    let sites = locate_variable(home.path(), "ANTHROPIC_AUTH_TOKEN");
    assert_eq!(sites.len(), 1);
    assert!(
        !sites[0].rewritable,
        "a command substitution must never be replaced with a literal"
    );
}

#[test]
fn fish_exports_are_understood() {
    let home = Home::new();
    home.write(
        ".config/fish/config.fish",
        "set -gx ANTHROPIC_BASE_URL https://relay.example.test\n",
    );
    let sites = locate_variable(home.path(), "ANTHROPIC_BASE_URL");
    assert_eq!(sites.len(), 1);
    assert_eq!(sites[0].value, "https://relay.example.test");
    assert!(sites[0].rewritable);

    // A fish variable that is not exported does not reach a child process.
    home.write(
        ".config/fish/config.fish",
        "set -g ANTHROPIC_BASE_URL https://relay.example.test\n",
    );
    assert!(locate_variable(home.path(), "ANTHROPIC_BASE_URL").is_empty());
}

#[test]
fn the_shell_decides_which_of_several_assignments_is_responsible() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "export ANTHROPIC_BASE_URL=https://old.example.test\nexport ANTHROPIC_BASE_URL=https://live.example.test\n",
    );

    let site = responsible_site(
        home.path(),
        "ANTHROPIC_BASE_URL",
        "https://live.example.test",
    )
    .expect("the line whose value the shell reports");
    assert_eq!(site.line, 2);

    // A value the files do not explain is not attributed to any of them.
    assert!(responsible_site(
        home.path(),
        "ANTHROPIC_BASE_URL",
        "https://elsewhere.example.test"
    )
    .is_none());
}

/// Two lines writing the same value: the later one is what the shell ends up
/// with, and so it is the one an edit has to change.
#[test]
fn the_last_matching_assignment_wins() {
    let home = Home::new();
    home.write(
        ".zshrc",
        "export ANTHROPIC_BASE_URL=https://same.example.test\nexport ANTHROPIC_BASE_URL=https://same.example.test\n",
    );
    let site = responsible_site(
        home.path(),
        "ANTHROPIC_BASE_URL",
        "https://same.example.test",
    )
    .expect("a site");
    assert_eq!(site.line, 2);
}

#[test]
fn quotes_are_removed_and_dynamic_values_are_flagged() {
    assert_eq!(unquote("'literal'"), ("literal".to_string(), true));
    assert_eq!(unquote("\"literal\""), ("literal".to_string(), true));
    assert_eq!(unquote("bare"), ("bare".to_string(), true));
    assert_eq!(unquote("\"$TOKEN\""), ("$TOKEN".to_string(), false));
    assert_eq!(unquote("`whoami`"), ("`whoami`".to_string(), false));
    assert_eq!(unquote(""), (String::new(), false));
}

#[test]
fn assignment_forms_are_recognised_and_nonsense_is_not() {
    assert_eq!(
        parse_assignment("export A_B=1"),
        Some(("A_B".to_string(), "1".to_string(), true))
    );
    assert_eq!(
        parse_assignment("  declare -x A_B=1"),
        Some(("A_B".to_string(), "1".to_string(), true))
    );
    assert_eq!(
        parse_assignment("A_B=1 # why"),
        Some(("A_B".to_string(), "1".to_string(), true))
    );
    assert_eq!(parse_assignment("if [ -n \"$A\" ]; then"), None);
    assert_eq!(parse_assignment("2BAD=1"), None);
    assert_eq!(
        parse_assignment("export PATH=$PATH:/usr/local/bin")
            .map(|(name, _, rewritable)| (name, rewritable)),
        Some(("PATH".to_string(), false))
    );
}

#[test]
fn a_source_argument_that_cannot_be_resolved_is_refused() {
    let home = Home::new();
    let parent = home.write(".zshrc", "");
    for argument in [
        "$SOME_OTHER_VAR/file.zsh",
        "`echo /tmp/x`",
        "~/.config/zsh/*.zsh",
        "",
    ] {
        assert!(
            resolve_sourced(argument, home.path(), &parent).is_none(),
            "{argument}"
        );
    }
}
