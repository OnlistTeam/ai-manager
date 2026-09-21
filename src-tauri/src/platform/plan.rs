use std::path::PathBuf;

use crate::platform::command::CommandSpec;

/// One command plus its failure tolerance. `ignore_failure` corresponds to the upstream `… || true`
/// (see `commands/misc.rs:2761`: in the Codex self-repair, a failed uninstall must not abort the
/// install that follows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleStep {
    pub spec: CommandSpec,
    pub ignore_failure: bool,
}

/// One "attempt": its inner steps run **in order and must all succeed** (except those with `ignore_failure`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleAttempt {
    pub steps: Vec<LifecycleStep>,
}

impl LifecycleAttempt {
    pub fn single(spec: CommandSpec) -> Self {
        Self {
            steps: vec![LifecycleStep {
                spec,
                ignore_failure: false,
            }],
        }
    }
}

/// The table of attempts tried in order: the next one is only tried after the previous one fails
/// **as a whole**, equivalent to the upstream `a || b` short-circuit chain.
/// Using structured attempts rather than stuffing `||` into a shell is a direct requirement of spec §14.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecyclePlan {
    pub attempts: Vec<LifecycleAttempt>,
}

impl LifecyclePlan {
    pub fn single(spec: CommandSpec) -> Self {
        Self {
            attempts: vec![LifecycleAttempt::single(spec)],
        }
    }

    /// No executable step at all. The caller must report an error and must never treat it as "ran and succeeded".
    pub fn is_empty(&self) -> bool {
        self.attempts.iter().all(|attempt| attempt.steps.is_empty())
    }
}

/// Uninstall has two mutually exclusive paths: what a package manager installed goes through its
/// command, and what an official installer installed can only be deleted file by file (upstream has
/// no uninstall at all; this knowledge is new in this product).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UninstallPlan {
    Command(LifecyclePlan),
    RemovePaths(Vec<PathBuf>),
}

#[cfg(test)]
mod tests {
    use super::{LifecycleAttempt, LifecyclePlan, LifecycleStep, UninstallPlan};
    use crate::platform::command::{AllowedProgram, CommandSpec};

    #[test]
    fn a_single_command_plan_is_one_attempt_of_one_step() {
        let spec = CommandSpec::new(AllowedProgram::Npm, vec!["--version".to_string()]);
        let plan = LifecyclePlan::single(spec.clone());
        assert_eq!(plan.attempts.len(), 1);
        assert_eq!(plan.attempts[0].steps.len(), 1);
        assert_eq!(plan.attempts[0].steps[0].spec, spec);
        assert!(!plan.attempts[0].steps[0].ignore_failure);
    }

    #[test]
    fn a_fallback_chain_keeps_the_attempt_order() {
        let primary = CommandSpec::new(AllowedProgram::ClaudeCode, vec!["update".to_string()]);
        let fallback = CommandSpec::new(AllowedProgram::Npm, vec!["install".to_string()]);
        let plan = LifecyclePlan {
            attempts: vec![
                LifecycleAttempt::single(primary.clone()),
                LifecycleAttempt::single(fallback.clone()),
            ],
        };
        assert_eq!(plan.attempts[0].steps[0].spec, primary);
        assert_eq!(plan.attempts[1].steps[0].spec, fallback);
        assert!(!plan.is_empty());
    }

    #[test]
    fn a_tolerated_step_is_marked_and_validates_like_any_other() {
        let step = LifecycleStep {
            spec: CommandSpec::new(AllowedProgram::Npm, vec!["uninstall".to_string()]),
            ignore_failure: true,
        };
        assert!(step.ignore_failure);
        assert!(step.spec.validate().is_ok());
    }

    #[test]
    fn an_empty_plan_is_detectable_so_callers_never_report_a_silent_success() {
        assert!(LifecyclePlan {
            attempts: Vec::new()
        }
        .is_empty());
        assert!(LifecyclePlan {
            attempts: vec![LifecycleAttempt { steps: Vec::new() }]
        }
        .is_empty());
    }

    #[test]
    fn an_uninstall_plan_is_either_commands_or_paths() {
        let commands = UninstallPlan::Command(LifecyclePlan::single(CommandSpec::new(
            AllowedProgram::Npm,
            vec!["uninstall".to_string()],
        )));
        let paths = UninstallPlan::RemovePaths(vec![std::path::PathBuf::from("/tmp/x")]);
        assert!(matches!(commands, UninstallPlan::Command(_)));
        assert!(matches!(paths, UninstallPlan::RemovePaths(ref p) if p.len() == 1));
    }
}
