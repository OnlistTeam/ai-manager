import { describe, expect, it } from "vitest";
import { codexProviderPresets } from "@/config/codexProviderPresets";

// Prefill policy (2026-08-15 official-docs inventory + Jason's same-day call
// of "form visibility first"):
// - Native Responses direct-connect presets: fill in the real
//   differentiated tier subset the vendor officially declares (including
//   verbatim copies of DeepSeek's official catalog mirror, and explicit
//   declarations for MiniMax/MiMo that match the template default -- the
//   form showing "unset" is more misleading than a stale/redundant
//   declaration);
// - Chat-routed presets (supportsEffort:false): tier values never enter the
//   wire; only fill in the two-state none/high when the preset declares a
//   real thinking toggle (supportsThinking + thinkingParam); when the
//   toggle isn't confirmed but the model does think, list only high to
//   represent a single always-thinking mode;
// - The backend applies per-row overrides to both paths' catalogs
//   (apply_codex_reasoning_level_override "Applies to every profile").
// The backend's codex_canonical_efforts silently drops unknown values --
// a typo in a preset won't error, it'll just silently make the Codex
// picker show fewer/wrong tiers, so allowlist validation must be
// backstopped at the test layer.
const CANONICAL_EFFORTS = [
  "none",
  "minimal",
  "low",
  "medium",
  "high",
  "xhigh",
  "max",
  "ultra",
];

function catalogModel(presetName: string, modelId: string) {
  const preset = codexProviderPresets.find((item) => item.name === presetName);
  expect(preset, `preset ${presetName}`).toBeDefined();
  const model = (preset?.modelCatalog ?? []).find(
    (item) => item.model === modelId,
  );
  expect(model, `${presetName} catalog model ${modelId}`).toBeDefined();
  return model!;
}

describe("Codex preset pre-filled reasoning levels", () => {
  // Every expected value corresponds to official-docs evidence (see the
  // comments in the presets file); verify the source before changing
  // either side. The 4th element = the expected explicit
  // defaultReasoningLevel: used for an official catalog's default, or to
  // keep the Chat catalog consistent with the preset config.toml's
  // explicit high (otherwise it might fall back to max).
  const EXPECTED: Array<[string, string, string[], string?]> = [
    // Volcengine's four official Codex integration docs agree: low/medium/high
    ["火山 Agent Plan", "ark-code-latest", ["low", "medium", "high"]],
    ["火山 Coding Plan", "ark-code-latest", ["low", "medium", "high"]],
    // Ark deep-thinking docs: this model's unrestricted general four tiers (minimal = thinking off, answer directly)
    [
      "Volcengine Doubao",
      "doubao-seed-2-1-pro-260628",
      ["minimal", "low", "medium", "high"],
    ],
    // Hunyuan officially enumerates low/high; hy3's open-source chat template raises directly on any other value
    ["Tencent Hunyuan", "hy3", ["low", "high"]],
    ["Tencent Hunyuan", "hy3-preview", ["low", "high"]],
    // Tencent Token Plan (subscription-line /plan endpoint): every tier
    // real-key tested (2026-08-31): glm-5.3 always thinks and strictly
    // enumerates low/high/max (medium/xhigh return 400 directly, and the
    // error message itself is the enum's source); kimi-k2.7-code(-highspeed)
    // only accepts thinking:enabled; minimax-m2.7 and domestic auto's
    // thinking-off is silently ignored (selecting none is a fake off) -> so
    // only high is listed; every other model's thinking toggle genuinely
    // works -> two states
    ["Tencent Token Plan", "tc-code-latest", ["none", "high"]],
    ["Tencent Token Plan", "hy3", ["none", "high"]],
    ["Tencent Token Plan", "minimax-m2.7", ["high"]],
    [
      "Tencent Token Plan Enterprise Pro",
      "glm-5.3",
      ["low", "high", "max"],
      "high",
    ],
    ["Tencent Token Plan Enterprise Pro", "kimi-k2.7-code", ["high"]],
    ["Tencent Token Plan Enterprise Pro", "auto", ["high"]],
    ["Tencent Token Plan Enterprise Pro", "glm-5.2", ["none", "high"]],
    // The international site's auto respects thinking-off (unlike domestic auto, which ignores it)
    ["Tencent Token Plan (Intl)", "auto", ["none", "high"]],
    ["Tencent Token Plan Enterprise Pro (Intl)", "auto", ["none", "high"]],
    [
      "Tencent Token Plan Enterprise Pro (Intl)",
      "glm-5.3",
      ["low", "high", "max"],
      "high",
    ],
    ["Tencent Token Plan Enterprise Lite", "auto", ["high"]],
    ["Tencent Token Plan Enterprise Lite (Intl)", "auto", ["none", "high"]],
    // LongCat has no adjustable tiers: the only effort evidence site-wide is the "high" in the official example
    ["Longcat", "LongCat-2.0", ["high"]],
    // xAI's Reasoning guide enumerates per model; grok-4.5 can't have thinking disabled, so there's no none
    ["xAI (Grok)", "grok-4.5", ["low", "medium", "high", "xhigh"]],
    ["xAI (Grok) OAuth", "grok-4.5", ["low", "medium", "high", "xhigh"]],
    // DeepSeek direct-connect copies the official catalog mirror verbatim
    // (Jason's call, 2026-08-15: form visibility first, accepting the risk
    // of a stale snapshot -- must be kept in sync when the official catalog
    // changes)
    ["DeepSeek", "deepseek-v4-flash", ["low", "high", "max"]],
    ["DeepSeek", "deepseek-v4-pro", ["low", "high", "max"]],
    // MiniMax/MiMo's official catalog is none/high (matches the template default; declared only for form visibility)
    ["MiniMax", "MiniMax-M3", ["none", "high"]],
    ["MiniMax en", "MiniMax-M3", ["none", "high"]],
    ["Xiaomi MiMo", "mimo-v2.5-pro", ["none", "high"]],
    ["Xiaomi MiMo", "mimo-v2.5", ["none", "high"]],
    ["Xiaomi MiMo Token Plan (China)", "mimo-v2.5-pro", ["none", "high"]],
    ["Xiaomi MiMo Token Plan (China)", "mimo-v2.5", ["none", "high"]],
    // Zhipu's official Codex integration page ships its own models.json
    // (docs.bigmodel.cn/cn/coding-plan/tool/codex,
    // docs.z.ai/devpack/tool/codex, checked 2026-09-04): glm-5.3's tiers are
    // low/high/max, default max (!= the backend's fallback template default
    // of high, hence the explicit declaration); glm-5-turbo's official
    // tiers are empty, default max -- cc-switch can't express an empty tier
    // set (it would fall back to the template's none/high, and none has no
    // conversion-layer backstop under native direct-connect, so it would be
    // sent as-is to a strict gateway), so per the official default it's
    // collapsed to the single tier max
    ["Zhipu GLM", "glm-5.3", ["low", "high", "max"], "max"],
    ["Zhipu GLM", "glm-5-turbo", ["max"]],
    ["Zhipu GLM en", "glm-5.3", ["low", "high", "max"], "max"],
    // SiliconFlow .com's M3: a platform-level enable_thinking boolean
    // toggle (the backend falls back to platform inference); M3 can
    // officially have thinking disabled -> two states
    ["SiliconFlow en", "MiniMaxAI/MiniMax-M3", ["none", "high"]],
    // SiliconFlow .cn: the platform only has high/max; the explicit high matches the preset config.
    ["SiliconFlow", "deepseek-ai/DeepSeek-V4-Flash", ["high", "max"], "high"],
    // Aggregator platforms haven't confirmed the toggle/effort for the new GLM yet; a single tier sends no reasoning control parameter.
    ["AtlasCloud", "zai-org/glm-5.2", ["high"]],
    ["Novita AI", "zai-org/glm-5.3", ["high"]],
    // NIM K3's official enum; the explicit high matches the preset config.
    ["Nvidia", "moonshotai/kimi-k3", ["low", "high", "max"], "high"],
    // Qianfan v2's official thinking:{type} (declaration already added) -> two states
    ["Baidu Qianfan Coding Plan", "qianfan-code-latest", ["none", "high"]],
    // Qianfan Token Plan: deepseek-v4-pro/v4-flash are in both the official
    // thinking and reasoning_effort lists (effort only has high/max as real
    // depth tiers); no default declared -> falls back to max, which happens
    // to match the platform's automatic behavior for complex agent-style
    // requests. glm-5.1 is only in the thinking list -> two states
    ["Baidu Qianfan Token Plan", "deepseek-v4-pro", ["none", "high", "max"]],
    ["Baidu Qianfan Token Plan", "deepseek-v4-flash", ["none", "high", "max"]],
    ["Baidu Qianfan Token Plan", "glm-5.1", ["none", "high"]],
    // BytePlus's international site has switched to native Responses; the
    // tiers = the official Codex docs' three tiers (cross-verified against
    // the same-origin domestic-site Volcengine dual Plan)
    ["BytePlus", "ark-code-latest", ["low", "medium", "high"]],
    // StepFun's official model pages (both sites) + reasoning guide:
    // 3.7-flash has three tiers (default medium), 2603 has two; the whole
    // family has no "no thinking" shape, so there's no none. The effort
    // value sent downstream is inferred by the backend per model
    // (2603=low_high, 3.7=passthrough)
    ["StepFun", "step-3.7-flash", ["low", "medium", "high"]],
    ["StepFun", "step-3.5-flash-2603", ["low", "high"]],
    ["StepFun en", "step-3.7-flash", ["low", "medium", "high"]],
    ["StepFun en", "step-3.5-flash-2603", ["low", "high"]],
    // Kimi Open Platform (native Responses direct-connect, default model
    // kimi-k3): k3's three tiers don't declare a default -- the native
    // template's default high is in the subset, so the backend keeps high
    // (= the preset config.toml's model_reasoning_effort); the official
    // default of max only applies to the API side's behavior when effort
    // isn't explicitly sent; k2.7-code always thinks and the official docs
    // note it doesn't support effort -> single tier. Neither can have
    // thinking turned off, so no none
    ["Kimi", "kimi-k3", ["low", "high", "max"]],
    ["Kimi", "kimi-k2.7-code", ["high"]],
    // Kimi Code endpoint (native Responses direct-connect): k3/k3-256k's
    // official models.json explicitly states default_reasoning_level
    // "high", same as the native template's fallback value -- copying the
    // official catalog's explicit declaration verbatim (form visibility
    // first, same precedent as MiniMax/MiMo) means it still has a 4th
    // expected element; kimi-for-coding(-highspeed) has Thinking always ON,
    // single tier
    ["Kimi For Coding", "kimi-for-coding", ["high"]],
    ["Kimi For Coding", "kimi-for-coding-highspeed", ["high"]],
    ["Kimi For Coding", "k3", ["low", "high", "max"], "high"],
    ["Kimi For Coding", "k3-256k", ["low", "high", "max"], "high"],
    // OpenCode Go (Zen gateway): the opencode client's variants() strictly
    // sends reasoning_effort per each model's reasoning_options declared on
    // models.dev (provider/transform.ts) -- both GLM5.3 variants have
    // low/high/max, Kimi K3 only has max. The proxy conversion layer clamps
    // per-model against that same table; GLM's default high matches the
    // preset config.
    ["OpenCode Go", "glm-5.3", ["low", "high", "max"], "high"],
    ["OpenCode Go", "glm-5.3-flash", ["low", "high", "max"], "high"],
    ["OpenCode Go", "kimi-k3", ["max"]],
    ["OpenCode Go", "deepseek-v4-pro", ["high", "max"]],
    ["OpenCode Go", "deepseek-v4-flash", ["low", "high", "max"]],
    // Qianwen's official Codex page publishes only one
    // model-catalog.local.json, and that metadata section comes before the
    // plan-tier pagination (help.aliyun.com/zh/model-studio/codex, checked
    // 2026-09-08): qwen3.8-max's tiers are low/medium/xhigh, default xhigh
    // (!= the template's fallback none/high, hence the explicit
    // declaration); pay-as-you-go and Token Plan share the same source file
    ["千问AI平台", "qwen3.8-max", ["low", "medium", "xhigh"], "xhigh"],
  ];

  it.each(EXPECTED)(
    "%s / %s declares the vendor-documented levels",
    (presetName, modelId, levels, expectedDefault) => {
      const model = catalogModel(presetName, modelId);
      expect(model.reasoningLevels).toEqual(levels);
      // See each preset's comments for the source of the explicit default; when not declared, the backend's fallback behavior still applies.
      expect(model.defaultReasoningLevel).toBe(expectedDefault);
    },
  );

  it("keeps deliberately-unfilled presets unfilled", () => {
    // OpenCode Go's MiMo has no effort declaration, matching the opencode
    // client (the proxy side doesn't send reasoning_effort when there's no
    // table entry). Whether ModelScope passes the thinking field through
    // hasn't been confirmed, so no fake two-state toggle is manufactured.
    const UNFILLED: Array<[string, string]> = [
      ["OpenCode Go", "mimo-v2.5-pro"],
      ["ModelScope", "ZhipuAI/GLM-5.2"],
      // StepFun's unsuffixed 3.5-flash: officially doesn't expose effort, single always-on thinking state
      ["StepFun", "step-3.5-flash"],
      ["StepFun en", "step-3.5-flash"],
      // Qianfan Token Plan: all three models are absent from the official
      // thinking list (2026-05-27 version), and no official integration
      // example sends the thinking field either -- no evidence, no
      // manufactured tiers
      ["Baidu Qianfan Token Plan", "deepseek-v4-flash-0731"],
      ["Baidu Qianfan Token Plan", "glm-5.2"],
      ["Baidu Qianfan Token Plan", "kimi-k2.6"],
    ];
    for (const [presetName, modelId] of UNFILLED) {
      const model = catalogModel(presetName, modelId);
      expect(
        model.reasoningLevels,
        `${presetName}/${modelId} must stay unfilled`,
      ).toBeUndefined();
    }
  });

  it("only ever declares canonical Codex efforts", () => {
    for (const preset of codexProviderPresets) {
      for (const model of preset.modelCatalog ?? []) {
        for (const level of model.reasoningLevels ?? []) {
          expect(
            CANONICAL_EFFORTS,
            `${preset.name}/${model.model} level "${level}"`,
          ).toContain(level);
        }
        if (model.defaultReasoningLevel !== undefined) {
          expect(model.reasoningLevels ?? []).toContain(
            model.defaultReasoningLevel,
          );
        }
      }
    }
  });
});
