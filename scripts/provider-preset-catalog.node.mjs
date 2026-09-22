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
import {
  ALLOWED_SERVICES,
  HOUSE_PRESETS,
} from "./provider-preset-policy.mjs";

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

test("catalog keeps eight-tool coverage without promotional metadata", async () => {
  const generated = await catalog();
  assert.equal(generated.version, 2);
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

/**
 * Shipping a preset points a user's key at an address this product chose for
 * them, so the catalogue may only name services the project is willing to
 * stand behind. The upstream sources still carry the third-party resellers;
 * this asserts that none of them reach a build.
 */
test("catalog ships only services on the allowlist", async () => {
  const generated = await catalog();
  const house = new Set(HOUSE_PRESETS.map((preset) => preset.serviceName));
  for (const preset of generated.presets) {
    assert.ok(
      ALLOWED_SERVICES.has(preset.serviceName) ||
        house.has(preset.serviceName),
      `${preset.tool}:${preset.serviceName} is not on the allowlist`,
    );
  }
  // Spot-checks from the 39 resellers the upstream catalogue lists, so a
  // regression in the filter fails loudly rather than by a count drifting.
  const serialized = JSON.stringify(generated);
  for (const reseller of [
    "9527CODE",
    "AiHubMix",
    "PackyCode",
    "SubRouter",
    "TheRouter",
    "QwenCloud",
    "千问AI平台",
  ]) {
    assert.doesNotMatch(
      serialized,
      new RegExp(reseller.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")),
      `${reseller} reached the shipped catalogue`,
    );
  }
});

/**
 * Leading the list is a placement, not a default: the dialog opens on
 * `defaultPresetId`, which stays the tool's own vendor.
 */
test("onList leads every tool without becoming its default", async () => {
  const generated = await catalog();
  for (const house of HOUSE_PRESETS) {
    const toolPresets = generated.presets.filter(
      (preset) => preset.tool === house.tool,
    );
    assert.equal(
      toolPresets[0].serviceName,
      "onList",
      `${house.tool} does not lead with onList`,
    );
    assert.equal(toolPresets[0].default, false);
    assert.equal(toolPresets[0].official, false);
    assert.ok(
      toolPresets.some((preset) => preset.default && preset.id !== "onlist"),
      `${house.tool} lost its vendor default`,
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
