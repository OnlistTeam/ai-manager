import { describe, expect, it } from "vitest";
import { providerPresets } from "@/config/claudeProviderPresets";
import { opencodeProviderPresets } from "@/config/opencodeProviderPresets";
import { hermesProviderPresets } from "@/config/hermesProviderPresets";
import {
  openclawProviderPresets,
  rebaseOpenClawSuggestedDefaults,
} from "@/config/openclawProviderPresets";

// Qianfan Token Plan Personal Edition (on sale replacing the Coding Plan since
// 2026-07-13; existing Coding Plan subscriptions remain usable until they
// expire, so the old preset stays alongside it). The Codex-side behavior is
// locked down by the codexChatProviderPresets and codexReasoningLevelPresets
// tests; this file covers the remaining five apps.
const PRESET_NAME = "Baidu Qianfan Token Plan";
const OPENAI_BASE = "https://qianfan.baidubce.com/v2/tokenplan/personal";
const ANTHROPIC_BASE =
  "https://qianfan.baidubce.com/anthropic/tokenplan/personal";
// Lineup = Token Plan main documentation (2026-08-14 edition); ernie-5.1 is officially marked as retired on 8/20 and is excluded
const MODEL_IDS = [
  "deepseek-v4-pro",
  "deepseek-v4-flash",
  "deepseek-v4-flash-0731",
  "glm-5.2",
  "glm-5.1",
  "kimi-k2.6",
];

describe("Baidu Qianfan Token Plan presets", () => {
  it("Claude preset points every model role at deepseek-v4-pro", () => {
    const preset = providerPresets.find((item) => item.name === PRESET_NAME);
    expect(preset).toBeDefined();

    const env = (preset?.settingsConfig as { env: Record<string, string> }).env;
    expect(env.ANTHROPIC_BASE_URL).toBe(ANTHROPIC_BASE);
    // Official Claude Code integration page (2026-07-30 edition): deepseek-v4-pro across every role
    for (const key of [
      "ANTHROPIC_MODEL",
      "ANTHROPIC_DEFAULT_HAIKU_MODEL",
      "ANTHROPIC_DEFAULT_SONNET_MODEL",
      "ANTHROPIC_DEFAULT_OPUS_MODEL",
    ]) {
      expect(env[key], key).toBe("deepseek-v4-pro");
    }
  });

  it("OpenCode preset carries the full Token Plan lineup", () => {
    const preset = opencodeProviderPresets.find(
      (item) => item.name === PRESET_NAME,
    );
    expect(preset).toBeDefined();
    expect(preset?.settingsConfig.npm).toBe("@ai-sdk/openai-compatible");
    expect(
      (preset?.settingsConfig.options as { baseURL: string }).baseURL,
    ).toBe(OPENAI_BASE);
    expect(Object.keys(preset?.settingsConfig.models ?? {})).toEqual(MODEL_IDS);
  });

  it("Hermes preset uses the OpenAI-compatible endpoint with v4-pro default", () => {
    const preset = hermesProviderPresets.find(
      (item) => item.name === PRESET_NAME,
    );
    expect(preset).toBeDefined();
    expect(preset?.settingsConfig.base_url).toBe(OPENAI_BASE);
    expect(preset?.settingsConfig.api_mode).toBe("chat_completions");
    expect(
      (preset?.settingsConfig.models ?? []).map((model) => model.id),
    ).toEqual(MODEL_IDS);
    expect(preset?.suggestedDefaults?.model).toEqual({
      default: "deepseek-v4-pro",
      provider: "qianfan_tokenplan",
    });
  });

  it("OpenClaw preset mirrors the official OpenClaw integration page", () => {
    const preset = openclawProviderPresets.find(
      (item) => item.name === PRESET_NAME,
    );
    expect(preset).toBeDefined();
    expect(preset?.settingsConfig.baseUrl).toBe(OPENAI_BASE);
    expect(preset?.settingsConfig.api).toBe("openai-completions");

    // Model entries = as-is from the official OpenClaw integration page (2026-07-22 edition).
    // The 98304 context window is the value officially designated for OpenClaw, which differs
    // from the platform's model-list page (1M) -- don't "correct" it to match the platform figure
    const model = preset?.settingsConfig.models?.[0];
    expect(model?.id).toBe("deepseek-v4-pro");
    expect(model?.contextWindow).toBe(98304);
    expect(model?.maxTokens).toBe(65536);
    expect(model?.cost).toEqual({
      input: 0.0025,
      output: 0.01,
      cacheRead: 0,
      cacheWrite: 0,
    });
  });

  it("rebases OpenClaw defaults to the submitted provider key", () => {
    const preset = openclawProviderPresets.find(
      (item) => item.name === PRESET_NAME,
    );
    expect(preset?.suggestedDefaults).toBeDefined();

    const rebased = rebaseOpenClawSuggestedDefaults(
      preset!.suggestedDefaults!,
      "my-qianfan",
    );
    expect(rebased.model?.primary).toBe("my-qianfan/deepseek-v4-pro");
    expect(rebased.modelCatalog).toHaveProperty("my-qianfan/deepseek-v4-pro");
  });
});
