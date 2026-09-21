import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import { parse } from "smol-toml";
import {
  buildProviderPresetCatalog,
  catalogFile,
  containsPlainSecret,
  serializeProviderPresetCatalog,
} from "./provider-preset-catalog.mjs";

let catalogPromise;
function catalog() {
  catalogPromise ??= buildProviderPresetCatalog();
  return catalogPromise;
}

test("Codex API templates use the upstream history bucket, not a product brand", async () => {
  const generated = await catalog();
  for (const preset of generated.presets.filter((item) => item.tool === "codex")) {
    const config = parse(preset.settingsConfig.config);
    assert.notEqual(config.model_provider, "ai_manager_openai", preset.id);
    if (preset.default) {
      assert.equal(config.model_provider, "custom");
      assert.equal(config.model_providers.custom.wire_api, "responses");
    }
  }
});

test("generated provider catalog stays synchronized with reviewed upstream presets", async () => {
  const generated = await catalog();
  const tracked = await readFile(catalogFile, "utf8");
  assert.equal(tracked, serializeProviderPresetCatalog(generated));
});

test("catalog keeps broad eight-tool coverage without promotional metadata", async () => {
  const generated = await catalog();
  assert.equal(generated.version, 2);
  assert.ok(generated.presets.length >= 400);
  assert.equal(generated.presets.filter((preset) => preset.official).length, 6);
  for (const tool of [
    "claude-code",
    "codex",
    "gemini-cli",
    "opencode",
    "grok-build",
    "openclaw",
    "hermes",
    "pi",
  ]) {
    const toolPresets = generated.presets.filter(
      (preset) => preset.tool === tool,
    );
    assert.ok(toolPresets.length > 0);
    assert.ok(
      toolPresets.some((preset) => preset.default),
      `${tool} has no default`,
    );
    assert.equal(
      toolPresets.filter((preset) => preset.default).length,
      1,
      `${tool} has multiple defaults`,
    );
  }

  const serialized = JSON.stringify(generated);
  assert.doesNotMatch(serialized, /cc[ -]?switch/i);
  assert.doesNotMatch(
    serialized,
    /partnerPromotionKey|primePartner|isPartner|affiliate|cn_official/,
  );
  assert.ok(
    generated.presets.some(
      (preset) => preset.serviceName === "DeepSeek" && preset.tool === "codex",
    ),
  );
  assert.ok(
    generated.presets.some(
      (preset) =>
        preset.serviceName === "OpenRouter" && preset.tool === "opencode",
    ),
  );
  assert.ok(
    generated.presets.some(
      (preset) =>
        preset.serviceName === "xAI (Grok)" &&
        preset.tool === "grok-build" &&
        preset.official &&
        preset.default,
    ),
  );
  for (const tool of ["openclaw", "pi"]) {
    assert.ok(
      generated.presets.some(
        (preset) =>
          preset.serviceName === "OpenRouter" &&
          preset.tool === tool &&
          preset.default &&
          !preset.official,
      ),
    );
  }
});

test("catalog exposes clean public origins and never carries a plaintext secret", async () => {
  const generated = await catalog();
  const identities = new Set();
  for (const preset of generated.presets) {
    const identity = `${preset.tool}:${preset.id}`;
    assert.ok(!identities.has(identity), identity);
    identities.add(identity);
    assert.match(preset.websiteUrl, /^https:\/\/[^/?#]+$/);
    assert.match(preset.apiKeyUrl, /^https:\/\/[^/?#]+$/);
    assert.equal(containsPlainSecret(preset.settingsConfig), false, identity);
  }
});
