import { describe, expect, it } from "vitest";
import { codexProviderPresets } from "@/config/codexProviderPresets";
import {
  extractCodexBaseUrl,
  extractCodexModelName,
  extractCodexWireApi,
} from "@/utils/providerConfigUtils";

const expectedChatPresets = new Map<
  string,
  { baseUrl: string; contextWindows: Record<string, number> }
>([
  // Volcengine Agent Plan / Coding Plan, BytePlus's international site
  // (coding/v3), Zhipu GLM, and both Kimi entries (Open Platform + Kimi
  // Code) have all switched to native Responses; see the native list below
  [
    "Baidu Qianfan Coding Plan",
    {
      baseUrl: "https://qianfan.baidubce.com/v2/coding",
      contextWindows: { "qianfan-code-latest": 131072 },
    },
  ],
  [
    "Baidu Qianfan Token Plan",
    {
      baseUrl: "https://qianfan.baidubce.com/v2/tokenplan/personal",
      contextWindows: {
        "deepseek-v4-pro": 1048576,
        "deepseek-v4-flash": 1048576,
        "deepseek-v4-flash-0731": 1048576,
        "glm-5.2": 1048576,
        "glm-5.1": 198000,
        "kimi-k2.6": 262144,
      },
    },
  ],
  [
    "StepFun",
    {
      baseUrl: "https://api.stepfun.com/step_plan/v1",
      contextWindows: {
        "step-3.7-flash": 262144,
        "step-3.5-flash-2603": 262144,
        "step-3.5-flash": 262144,
      },
    },
  ],
  [
    "StepFun en",
    {
      baseUrl: "https://api.stepfun.ai/step_plan/v1",
      contextWindows: {
        "step-3.7-flash": 262144,
        "step-3.5-flash-2603": 262144,
        "step-3.5-flash": 262144,
      },
    },
  ],
  [
    "ModelScope",
    {
      baseUrl: "https://api-inference.modelscope.cn/v1",
      contextWindows: { "ZhipuAI/GLM-5.2": 200000 },
    },
  ],
  [
    "BaiLing",
    {
      baseUrl: "https://api.tbox.cn/api/llm/v1",
      contextWindows: { "Ling-2.6-1T": 262144 },
    },
  ],
  [
    "SiliconFlow",
    {
      baseUrl: "https://api.siliconflow.cn/v1",
      contextWindows: { "deepseek-ai/DeepSeek-V4-Flash": 1048576 },
    },
  ],
  [
    "SiliconFlow en",
    {
      baseUrl: "https://api.siliconflow.com/v1",
      contextWindows: { "MiniMaxAI/MiniMax-M3": 1048576 },
    },
  ],
  [
    "AtlasCloud",
    {
      baseUrl: "https://api.atlascloud.ai/v1",
      contextWindows: { "zai-org/glm-5.2": 1048576 },
    },
  ],
  [
    "Novita AI",
    {
      baseUrl: "https://api.novita.ai/openai/v1",
      contextWindows: { "zai-org/glm-5.3": 1048576 },
    },
  ],
  [
    "Nvidia",
    {
      baseUrl: "https://integrate.api.nvidia.com/v1",
      contextWindows: { "moonshotai/kimi-k3": 1048576 },
    },
  ],
  [
    "OpenCode Go",
    {
      baseUrl: "https://opencode.ai/zen/go/v1",
      contextWindows: {
        "glm-5.3": 1000000,
        "glm-5.3-flash": 1000000,
        "kimi-k3": 1048576,
        "deepseek-v4-pro": 1048576,
        "deepseek-v4-flash": 1048576,
        "mimo-v2.5-pro": 1048576,
      },
    },
  ],
]);

describe("Codex Chat provider presets", () => {
  it("drops prompt cache routing once Kimi Coding is direct-connect", () => {
    // promptCacheRouting is only consumed by the Responses->Chat conversion
    // layer (the forwarder re-injects prompt_cache_key after converting).
    // Native direct-connect has Codex send prompt_cache_key itself, so
    // leaving this flag on would wrongly suggest the card still needs
    // routing takeover.
    const preset = codexProviderPresets.find(
      (item) => item.name === "Kimi For Coding",
    );

    expect(preset?.apiFormat).toBe("openai_responses");
    expect(preset?.promptCacheRouting).toBeUndefined();
  });

  it("marks migrated Chat Completions presets for local routing", () => {
    for (const [name, expected] of expectedChatPresets) {
      const preset = codexProviderPresets.find((item) => item.name === name);

      expect(preset, `${name} preset`).toBeDefined();
      expect(preset?.apiFormat).toBe("openai_chat");
      expect(extractCodexBaseUrl(preset?.config)).toBe(expected.baseUrl);
      expect(extractCodexWireApi(preset?.config)).toBe("responses");
      expect(preset?.endpointCandidates).toContain(expected.baseUrl);
      expect(preset?.modelCatalog?.length).toBeGreaterThan(0);
      expect(extractCodexModelName(preset?.config)).toBe(
        preset?.modelCatalog?.[0]?.model,
      );
      expect(
        Object.fromEntries(
          (preset?.modelCatalog ?? []).map((model) => [
            model.model,
            model.contextWindow,
          ]),
        ),
      ).toEqual(expected.contextWindows);
    }
  });

  it("uses native Responses API for migrated CN providers without local route mapping", () => {
    const nativeResponsesPresets = new Map<
      string,
      { baseUrl?: string; contextWindows: Record<string, number> }
    >([
      // The official Codex docs confirm both Agent Plan /api/plan/v3 and
      // Coding Plan /api/coding/v3 support the Responses API; same for
      // BytePlus's international site coding/v3
      // (docs.byteplus.com/en/docs/ModelArk/2556056, verified 2026-08-15)
      ["火山 Agent Plan", { contextWindows: { "ark-code-latest": 256000 } }],
      ["火山 Coding Plan", { contextWindows: { "ark-code-latest": 256000 } }],
      ["BytePlus", { contextWindows: { "ark-code-latest": 256000 } }],
      [
        "Volcengine Doubao",
        { contextWindows: { "doubao-seed-2-1-pro-260628": 262144 } },
      ],
      ["千问AI平台", { contextWindows: { "qwen3.8-max": 983616 } }],
      // Tencent TokenHub's official Codex docs confirm hy3 uses native
      // Responses (2026-07-14)
      [
        "Tencent Hunyuan",
        { contextWindows: { hy3: 256000, "hy3-preview": 256000 } },
      ],
      // DeepSeek's official Codex docs confirm deepseek-v4-flash uses
      // native Responses; the catalog is generated by the backend by
      // mirroring the official models.json for the deepseek.com host
      [
        "DeepSeek",
        {
          contextWindows: {
            "deepseek-v4-flash": 1048576,
            "deepseek-v4-pro": 1048576,
          },
        },
      ],
      ["Longcat", { contextWindows: { "LongCat-2.0": 1048576 } }],
      ["MiniMax", { contextWindows: { "MiniMax-M3": 1000000 } }],
      ["MiniMax en", { contextWindows: { "MiniMax-M3": 1000000 } }],
      [
        "Xiaomi MiMo",
        {
          contextWindows: {
            "mimo-v2.5-pro": 1048576,
            "mimo-v2.5": 1048576,
          },
        },
      ],
      [
        "Xiaomi MiMo Token Plan (China)",
        {
          contextWindows: {
            "mimo-v2.5-pro": 1048576,
            "mimo-v2.5": 1048576,
          },
        },
      ],
      // Zhipu has three separate endpoints (Anthropic /api/anthropic, Chat
      // /api/coding/paas/v4, Responses /api/v1); the docs explicitly state
      // the wrong endpoint can't draw on Coding Plan package quota -- the
      // native Responses preset must be locked to /api/v1 (#6944;
      // docs.bigmodel.cn/cn/coding-plan/tool/codex and
      // docs.z.ai/devpack/tool/codex ship their own models.json, checked
      // 2026-09-04: domestic site has glm-5.3 + glm-5-turbo, international
      // site has glm-5.3 only)
      [
        "Zhipu GLM",
        {
          baseUrl: "https://open.bigmodel.cn/api/v1",
          contextWindows: { "glm-5.3": 1048576, "glm-5-turbo": 204800 },
        },
      ],
      [
        "Zhipu GLM en",
        {
          baseUrl: "https://api.z.ai/api/v1",
          contextWindows: { "glm-5.3": 1048576 },
        },
      ],
      // Both of Kimi's official Codex integration docs require
      // wire_api = "responses" and explicitly state the service natively
      // implements the Responses API, no local routing or protocol
      // conversion needed (platform.kimi.com/docs/guide/codex-kimi.md and
      // kimi.com/code/docs/third-party-tools/codex.html, re-verified with a
      // real-key probe on 2026-09-09)
      [
        "Kimi",
        {
          baseUrl: "https://api.moonshot.cn/v1",
          contextWindows: { "kimi-k3": 1048576, "kimi-k2.7-code": 262144 },
        },
      ],
      [
        "Kimi For Coding",
        {
          baseUrl: "https://api.kimi.com/coding/v1",
          contextWindows: {
            "kimi-for-coding": 262144,
            "kimi-for-coding-highspeed": 262144,
            k3: 1048576,
            "k3-256k": 262144,
          },
        },
      ],
    ]);

    for (const [name, expected] of nativeResponsesPresets) {
      const preset = codexProviderPresets.find((item) => item.name === name);

      expect(preset, `${name} preset`).toBeDefined();
      expect(preset?.apiFormat).toBe("openai_responses");
      if (expected.baseUrl) {
        // A direct-connect preset's base_url must be the vendor's Responses
        // endpoint itself (not the same site's Chat endpoint);
        // endpointCandidates shares the same path tier as the primary address
        expect(extractCodexBaseUrl(preset?.config)).toBe(expected.baseUrl);
        expect(preset?.endpointCandidates).toContain(expected.baseUrl);
        expect(extractCodexModelName(preset?.config)).toBe(
          preset?.modelCatalog?.[0]?.model,
        );
      }
      // Native Responses presets now carry a modelCatalog: when cc-switch
      // is directly connected, this is used to generate ~/.codex's
      // model-catalogs.json (shell_command edits, no freeform apply_patch
      // sent). Carrying a catalog no longer forces “local routing mapping”
      // on -- the frontend has decoupled this based on apiFormat
      // (openai_responses doesn't enable takeover by default).
      expect((preset?.modelCatalog ?? []).length).toBeGreaterThan(0);
      expect(
        Object.fromEntries(
          (preset?.modelCatalog ?? []).map((model) => [
            model.model,
            model.contextWindow,
          ]),
        ),
      ).toEqual(expected.contextWindows);
      // Native (direct-connect) doesn't go through Chat conversion, so codexChatReasoning isn't needed.
      expect(preset?.codexChatReasoning).toBeUndefined();
    }
  });

  it("ships per-model reasoningLevels for OpenCode Go mirroring models.dev", () => {
    // The Zen gateway's legal effort tiers vary per model (models.dev
    // reasoning_options, 2026-09-10): a uniform union mapping would send
    // high to kimi-k3, which only declares max. This test pins the
    // per-model table to prevent regressions.
    const preset = codexProviderPresets.find(
      (item) => item.name === "OpenCode Go",
    );

    expect(preset, "OpenCode Go preset").toBeDefined();
    expect(preset?.codexChatReasoning?.effortValueMode).toBe("zen");
    expect(
      Object.fromEntries(
        (preset?.modelCatalog ?? []).map((model) => [
          model.model,
          model.reasoningLevels ?? null,
        ]),
      ),
    ).toEqual({
      "glm-5.3": ["low", "high", "max"],
      "glm-5.3-flash": ["low", "high", "max"],
      "kimi-k3": ["max"],
      "deepseek-v4-pro": ["high", "max"],
      "deepseek-v4-flash": ["low", "high", "max"],
      "mimo-v2.5-pro": null,
    });
  });
});
