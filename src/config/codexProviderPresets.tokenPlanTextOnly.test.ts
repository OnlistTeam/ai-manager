import { describe, expect, it } from "vitest";

import { codexProviderPresets } from "./codexProviderPresets";

// DeepSeek's official endpoint now routes `deepseek-v4-flash` to the
// image-capable V4.1 Flash, so the global text-only allowlist no longer
// carries that id (#7283). The V4 deployments hosted on Qianfan / Tencent
// Token Plan are still text-only (Qianfan Coding Plan's docs explicitly
// state attachments return 400), and this must be backstopped by an
// explicit declaration on the preset row, otherwise it fails open into
// "can attach images".
const textOnlyDeepSeekHosts = [
  "Baidu Qianfan Token Plan",
  "Tencent Token Plan Enterprise Pro",
  "Tencent Token Plan Enterprise Pro (Intl)",
];

describe("third-party Token Plan DeepSeek V4 rows stay text-only", () => {
  it.each(textOnlyDeepSeekHosts)("%s declares text-only", (name) => {
    const preset = codexProviderPresets.find((p) => p.name === name);
    expect(preset, name).toBeDefined();
    const rows = (preset!.modelCatalog ?? []).filter((row) =>
      row.model.startsWith("deepseek-v4-"),
    );
    expect(rows.length, `${name} carries DeepSeek V4 rows`).toBeGreaterThan(0);
    for (const row of rows) {
      expect(row.inputModalities, `${name}/${row.model}`).toEqual(["text"]);
    }
  });

  it("the official DeepSeek preset keeps fail-open (no modality override)", () => {
    const preset = codexProviderPresets.find((p) => p.name === "DeepSeek");
    expect(preset).toBeDefined();
    for (const row of preset!.modelCatalog ?? []) {
      expect(row.inputModalities, row.model).toBeUndefined();
    }
  });
});
