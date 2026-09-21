import { describe, expect, it } from "vitest";
import { codexProviderPresets } from "@/config/codexProviderPresets";
import { openclawProviderPresets } from "@/config/openclawProviderPresets";

// Regression: the same doubao model declares contextWindow in both the Codex
// catalog and the OpenClaw settingsConfig, and the two must match. There was
// previously a drift where OpenClaw had 128000 while Codex had 262144,
// leaving OpenClaw users with a too-small context window that compressed or
// truncated long contexts prematurely.
describe("Volcengine Doubao preset consistency across apps", () => {
  const DOUBAO_MODEL_ID = "doubao-seed-2-1-pro-260628";
  const EXPECTED_CONTEXT_WINDOW = 262144;

  it("keeps the doubao context window in sync between OpenClaw and Codex", () => {
    const codexPreset = codexProviderPresets.find(
      (item) => item.name === "Volcengine Doubao",
    );
    const codexModel = (codexPreset?.modelCatalog ?? []).find(
      (model) => model.model === DOUBAO_MODEL_ID,
    );
    expect(codexModel, "Codex Volcengine Doubao catalog model").toBeDefined();
    expect(codexModel?.contextWindow).toBe(EXPECTED_CONTEXT_WINDOW);

    const openclawPreset = openclawProviderPresets.find(
      (item) => item.name === "Volcengine Doubao",
    );
    const openclawModel = (openclawPreset?.settingsConfig.models ?? []).find(
      (model) => model.id === DOUBAO_MODEL_ID,
    );
    expect(openclawModel, "OpenClaw Volcengine Doubao model").toBeDefined();
    expect(openclawModel?.contextWindow).toBe(EXPECTED_CONTEXT_WINDOW);

    // Changing either side alone would break this equality -- this is the real anti-drift assertion.
    expect(openclawModel?.contextWindow).toBe(codexModel?.contextWindow);
  });
});
