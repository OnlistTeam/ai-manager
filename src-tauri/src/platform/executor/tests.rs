use super::{
    retain_tail, CommandCancellation, CommandExecutor, CommandStream, SystemExecutor,
    RETAINED_OUTPUT_MAX_BYTES,
};
use crate::domain::ErrorCode;
use crate::platform::command::{AllowedProgram, CommandSpec};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn executor() -> SystemExecutor {
    SystemExecutor
}

#[tokio::test]
async fn a_successful_command_reports_stdout_and_exit_zero() {
    let output = executor()
        .execute(CommandSpec::bash_script("printf 'hello world'"))
        .await
        .expect("bash runs");
    assert!(output.success);
    assert_eq!(output.exit_code, Some(0));
    assert_eq!(output.stdout, "hello world");
    assert_eq!(output.stderr, "");
}

#[tokio::test]
async fn a_failing_command_is_ok_with_success_false_and_captured_stderr() {
    let output = executor()
        .execute(CommandSpec::bash_script("printf oops >&2; exit 3"))
        .await
        .expect("a non-zero exit is not an executor error");
    assert!(!output.success);
    assert_eq!(output.exit_code, Some(3));
    assert_eq!(output.stderr, "oops");
    assert_eq!(output.stdout, "");
}

#[tokio::test]
async fn output_larger_than_the_pipe_buffer_does_not_deadlock() {
    let spec =
        CommandSpec::bash_script("dd if=/dev/zero bs=1024 count=200 2>/dev/null | tr '\\000' 'a'")
            .with_timeout(Duration::from_secs(30));
    let output = executor().execute(spec).await.expect("bash runs");
    assert!(output.success);
    assert_eq!(output.stdout.len(), 204_800);
}

/// An installer that floods stdout must not grow the process by the size of
/// its output: only a bounded tail is retained, marked as truncated.
#[tokio::test]
async fn runaway_output_keeps_only_a_bounded_marked_tail() {
    let spec =
        CommandSpec::bash_script("yes | head -n 3000000").with_timeout(Duration::from_secs(60));
    let output = executor().execute(spec).await.expect("bash runs");
    assert!(output.success);
    assert!(output.stdout.starts_with("[earlier output dropped]"));
    assert!(output.stdout.ends_with('y'));
    assert!(output.stdout.len() <= RETAINED_OUTPUT_MAX_BYTES + 32);
}

/// Past the limit the buffer is cut back to half of it in one move, on a
/// line boundary, so the retained tail always holds between half the limit
/// and the limit and the shift is not paid once per line.
#[test]
fn the_retained_tail_drops_whole_lines_from_the_front() {
    let mut output = Vec::new();
    let mut truncated = false;
    for _ in 0..3 {
        retain_tail(&mut output, b"aaaa\n", 10, &mut truncated);
    }
    assert_eq!(output, b"aaaa\n");
    assert!(truncated);

    // A single over-long line keeps its tail rather than vanishing.
    let mut output = Vec::new();
    let mut truncated = false;
    retain_tail(&mut output, b"0123456789abcdef\n", 8, &mut truncated);
    assert_eq!(output, b"def\n");
    assert!(truncated);

    // Under the limit nothing is dropped or marked.
    let mut output = Vec::new();
    let mut truncated = false;
    retain_tail(&mut output, b"short\n", 10, &mut truncated);
    assert_eq!(output, b"short\n");
    assert!(!truncated);
}

#[tokio::test]
async fn a_command_past_its_timeout_is_killed_and_reported() {
    let spec = CommandSpec::bash_script("sleep 30").with_timeout(Duration::from_millis(250));
    let started = std::time::Instant::now();
    let error = executor()
        .execute(spec)
        .await
        .expect_err("a hung command must fail");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.message_key, "error.command.timeout");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the executor must not wait for the child to exit on its own"
    );
}

#[tokio::test]
async fn cancellation_before_spawn_is_confirmed_without_running_the_command() {
    let cancellation = CommandCancellation::default();
    assert!(cancellation.request());
    let error = executor()
        .execute_streaming_cancellable(
            CommandSpec::bash_script("sleep 30"),
            Arc::new(|_| {}),
            cancellation.clone(),
        )
        .await
        .expect_err("the requested command must not start");
    assert_eq!(error.message_key, "error.operation.cancelled");
    assert!(cancellation.is_confirmed());
}

#[tokio::test]
async fn cancellation_kills_a_running_process_tree_before_it_completes() {
    let cancellation = CommandCancellation::default();
    let requested = cancellation.clone();
    let task = tokio::spawn(async move {
        executor()
            .execute_streaming_cancellable(
                CommandSpec::bash_script("sleep 30 & wait"),
                Arc::new(|_| {}),
                cancellation,
            )
            .await
    });
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(requested.request());
    let error = task
        .await
        .expect("executor task joins")
        .expect_err("running tree is cancelled");
    assert_eq!(error.message_key, "error.operation.cancelled");
    assert!(requested.is_confirmed());
}

#[tokio::test]
async fn env_entries_reach_the_child_process() {
    let spec = CommandSpec::bash_script("printf %s \"$AIM_EXECUTOR_PROBE\"")
        .with_env("AIM_EXECUTOR_PROBE", "from-spec");
    let output = executor().execute(spec).await.expect("bash runs");
    assert_eq!(output.stdout, "from-spec");
}

#[tokio::test]
async fn streaming_reports_stdout_and_stderr_without_changing_the_result() {
    let chunks = Arc::new(Mutex::new(Vec::new()));
    let sink = chunks.clone();
    let output = executor()
        .execute_streaming(
            CommandSpec::bash_script("printf 'first\\n'; printf 'warning\\n' >&2"),
            Arc::new(move |chunk| sink.lock().expect("chunks lock").push(chunk)),
        )
        .await
        .expect("bash runs");

    assert_eq!(output.stdout, "first");
    assert_eq!(output.stderr, "warning");
    let chunks = chunks.lock().expect("chunks lock");
    assert!(chunks
        .iter()
        .any(|chunk| chunk.stream == CommandStream::Stdout && chunk.text.contains("first")));
    assert!(chunks
        .iter()
        .any(|chunk| chunk.stream == CommandStream::Stderr && chunk.text.contains("warning")));
}

#[tokio::test]
async fn a_missing_program_path_surfaces_as_tool_not_found() {
    let spec = CommandSpec::new(AllowedProgram::Npm, vec!["--version".to_string()])
        .with_program_path(std::path::PathBuf::from("/nonexistent-aim-probe/bin/npm"));
    let error = executor()
        .execute(spec)
        .await
        .expect_err("a missing binary must fail");
    assert_eq!(error.code, ErrorCode::ToolNotFound);
    assert_eq!(error.message_key, "error.command.programNotFound");
}

#[tokio::test]
async fn the_spec_is_validated_before_anything_is_spawned() {
    let spec = CommandSpec::bash_script("true").with_timeout(Duration::from_millis(0));
    let error = executor()
        .execute(spec)
        .await
        .expect_err("an invalid spec must fail");
    assert_eq!(error.message_key, "error.commandSpec.timeoutZero");
}

#[tokio::test]
async fn errors_never_leak_the_raw_command_output() {
    let spec = CommandSpec::new(AllowedProgram::Npm, vec!["config".to_string()])
        .with_program_path(std::path::PathBuf::from("/nonexistent-aim-probe/bin/npm"))
        .with_sensitive_args(vec![0]);
    let error = executor().execute(spec).await.expect_err("must fail");
    let technical = error.technical_message.unwrap_or_default();
    assert!(technical.contains("***"), "sensitive args stay redacted");
    assert!(!technical.contains("config"));
}
