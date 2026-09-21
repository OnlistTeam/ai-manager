import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { chmod, mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

const root = new URL("../", import.meta.url);
const script = new URL("scripts/notarize-macos.sh", root);
const submissionId = "12345678-1234-1234-1234-123456789abc";

async function fixture(mode) {
  const directory = await mkdtemp(join(tmpdir(), "ai-manager-notary-"));
  const artifact = join(directory, "AI Manager.zip");
  const logs = join(directory, "logs");
  const calls = join(directory, "calls.txt");
  const counter = join(directory, "counter.txt");
  const fake = join(directory, "notarytool");
  await writeFile(artifact, "signed app fixture");
  await writeFile(
    fake,
    `#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$1" >> "$FAKE_CALLS"
case "$1" in
  submit)
    printf '{"id":"${submissionId}"}\n'
    ;;
  wait)
    count=0
    if [[ -f "$FAKE_COUNTER" ]]; then count=$(cat "$FAKE_COUNTER"); fi
    count=$((count + 1))
    printf '%s' "$count" > "$FAKE_COUNTER"
    if [[ "$FAKE_MODE" == "transient" && "$count" -eq 1 ]]; then
      printf 'temporary network failure\n' >&2
      exit 1
    fi
    if [[ "$FAKE_MODE" == "auth-error" ]]; then
      printf 'authentication credentials are invalid\n' >&2
      exit 1
    fi
    if [[ "$FAKE_MODE" == "invalid" ]]; then
      printf '{"status":"Invalid"}\n'
    else
      printf '{"status":"Accepted"}\n'
    fi
    ;;
  log)
    printf '{"status":"%s"}\n' "$FAKE_MODE" > "$3"
    ;;
  *)
    exit 2
    ;;
esac
`,
  );
  await chmod(fake, 0o755);
  return { artifact, calls, counter, directory, fake, logs, mode };
}

function run(input) {
  return spawnSync("/bin/bash", [script.pathname, input.artifact, input.logs], {
    encoding: "utf8",
    env: {
      ...process.env,
      AI_MANAGER_NOTARYTOOL_BIN: input.fake,
      AI_MANAGER_NOTARY_RETRY_DELAY_SECONDS: "0",
      APPLE_ID: "release@example.invalid",
      APPLE_NOTARIZATION_METHOD: "apple-id",
      APPLE_PASSWORD: "fixture-password",
      APPLE_TEAM_ID: "FIXTURETEAM",
      FAKE_CALLS: input.calls,
      FAKE_COUNTER: input.counter,
      FAKE_MODE: input.mode,
    },
  });
}

test(
  "accepted notarization submits once and preserves the final service log",
  { skip: process.platform === "win32" },
  async () => {
    const input = await fixture("accepted");
    const result = run(input);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual((await readFile(input.calls, "utf8")).trim().split("\n"), [
      "submit",
      "wait",
      "log",
    ]);
    assert.match(
      await readFile(join(input.logs, "final.json"), "utf8"),
      /accepted/,
    );
  },
);

test(
  "a completed invalid submission is never retried",
  { skip: process.platform === "win32" },
  async () => {
    const input = await fixture("invalid");
    const result = run(input);
    assert.notEqual(result.status, 0);
    assert.deepEqual((await readFile(input.calls, "utf8")).trim().split("\n"), [
      "submit",
      "wait",
      "log",
    ]);
    assert.match(result.stderr, /will not be retried/);
  },
);

test(
  "transient polling failure retries the existing submission only",
  { skip: process.platform === "win32" },
  async () => {
    const input = await fixture("transient");
    const result = run(input);
    assert.equal(result.status, 0, result.stderr);
    assert.deepEqual((await readFile(input.calls, "utf8")).trim().split("\n"), [
      "submit",
      "wait",
      "wait",
      "log",
    ]);
  },
);

test(
  "deterministic polling failure is never retried",
  { skip: process.platform === "win32" },
  async () => {
    const input = await fixture("auth-error");
    const result = run(input);
    assert.notEqual(result.status, 0);
    assert.deepEqual((await readFile(input.calls, "utf8")).trim().split("\n"), [
      "submit",
      "wait",
      "log",
    ]);
    assert.match(result.stderr, /deterministically; it will not be retried/);
    assert.match(
      await readFile(join(input.logs, "wait-1-error.log"), "utf8"),
      /credentials are invalid/,
    );
  },
);
