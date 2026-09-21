#!/usr/bin/env bash

set -euo pipefail

artifact=${1:?"usage: notarize-macos.sh <artifact> <log-directory>"}
log_directory=${2:?"usage: notarize-macos.sh <artifact> <log-directory>"}

if [[ ! -f "$artifact" ]]; then
  echo "Notarization artifact does not exist: $artifact" >&2
  exit 1
fi

mkdir -p "$log_directory"
umask 077

run_notarytool() {
  if [[ -n "${AI_MANAGER_NOTARYTOOL_BIN:-}" ]]; then
    "$AI_MANAGER_NOTARYTOOL_BIN" "$@"
  else
    xcrun notarytool "$@"
  fi
}

is_transient_wait_failure() {
  LC_ALL=C grep -Eiq \
    'tim(e|ed)[ -]?out|timeout|temporar|try again|connection|network|transport|TLS|HTTP[^0-9]*(408|425|429|5[0-9]{2})|service unavailable|gateway' \
    "$@"
}

json_value() {
  node -e '
    const fs = require("node:fs");
    const value = JSON.parse(fs.readFileSync(process.argv[1], "utf8"))[process.argv[2]];
    if (typeof value !== "string" || value.length === 0) process.exit(2);
    process.stdout.write(value);
  ' "$1" "$2"
}

auth_args=()
case "${APPLE_NOTARIZATION_METHOD:-}" in
  apple-id)
    : "${APPLE_ID:?APPLE_ID is required for apple-id notarization}"
    : "${APPLE_PASSWORD:?APPLE_PASSWORD is required for apple-id notarization}"
    : "${APPLE_TEAM_ID:?APPLE_TEAM_ID is required for apple-id notarization}"
    auth_args=(
      --apple-id "$APPLE_ID"
      --password "$APPLE_PASSWORD"
      --team-id "$APPLE_TEAM_ID"
    )
    ;;
  app-store-connect)
    : "${APPLE_API_KEY_PATH:?APPLE_API_KEY_PATH is required for API-key notarization}"
    : "${APPLE_API_KEY:?APPLE_API_KEY is required for API-key notarization}"
    : "${APPLE_API_ISSUER:?APPLE_API_ISSUER is required for API-key notarization}"
    auth_args=(
      --key "$APPLE_API_KEY_PATH"
      --key-id "$APPLE_API_KEY"
      --issuer "$APPLE_API_ISSUER"
    )
    ;;
  *)
    echo "APPLE_NOTARIZATION_METHOD must be apple-id or app-store-connect" >&2
    exit 1
    ;;
esac

submit_log="$log_directory/submit.json"
submit_error_log="$log_directory/submit-error.log"
# Submit exactly once: authentication, configuration, and preflight failures are deterministic and
# must not be hidden by rebuilding or resubmitting the artifact.
if ! run_notarytool submit "$artifact" "${auth_args[@]}" \
  --no-wait --no-progress --output-format json \
  >"$submit_log" 2>"$submit_error_log"; then
  echo "Notarization submission failed; see $log_directory" >&2
  exit 1
fi

submission_id=$(json_value "$submit_log" id)
if [[ ! "$submission_id" =~ ^[0-9A-Fa-f-]{36}$ ]]; then
  echo "Notarization service returned an invalid submission id" >&2
  exit 1
fi

wait_timeout=${AI_MANAGER_NOTARY_WAIT_TIMEOUT:-20m}
retry_delay=${AI_MANAGER_NOTARY_RETRY_DELAY_SECONDS:-15}

# Only polling is retried. The already-uploaded artifact keeps one submission id, and every wait
# response plus the final service log remains available as workflow evidence.
for attempt in 1 2 3; do
  wait_log="$log_directory/wait-$attempt.json"
  wait_error_log="$log_directory/wait-$attempt-error.log"
  if run_notarytool wait "$submission_id" "${auth_args[@]}" \
    --timeout "$wait_timeout" --no-progress --output-format json \
    >"$wait_log" 2>"$wait_error_log"; then
    status=$(json_value "$wait_log" status)
    run_notarytool log "$submission_id" "$log_directory/final.json" \
      "${auth_args[@]}" >/dev/null || true
    if [[ "$status" == "Accepted" ]]; then
      exit 0
    fi

    echo "Notarization completed with status $status; it will not be retried" >&2
    exit 1
  fi

  if ! is_transient_wait_failure "$wait_log" "$wait_error_log"; then
    run_notarytool log "$submission_id" "$log_directory/final.json" \
      "${auth_args[@]}" >/dev/null || true
    echo "Notarization polling failed deterministically; it will not be retried" >&2
    exit 1
  fi

  if [[ "$attempt" -lt 3 ]]; then
    sleep "$retry_delay"
  fi
done

run_notarytool log "$submission_id" "$log_directory/final.json" \
  "${auth_args[@]}" >/dev/null || true
echo "Notarization polling failed after three bounded attempts" >&2
exit 1
