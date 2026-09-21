import { describe, expect, it } from "vitest";
import {
  codexProviderPresets,
  generateThirdPartyConfig,
} from "./codexProviderPresets";

describe("codexProviderPresets managed OAuth snapshots", () => {
  // Managed OAuth cards have no static key: requires_openai_auth = true
  // gets rejected by the backend's keyless safety gate when switching
  // (provider.codex.config.official_auth_fallback). The backend write
  // layer also forces existing cards back to false, so presets must never
  // carry true from the source.
  it("OAuth presets never declare the auth.json fallback", () => {
    const oauthPresets = codexProviderPresets.filter(
      (preset) => preset.requiresOAuth,
    );
    expect(oauthPresets.length).toBeGreaterThan(0);
    for (const preset of oauthPresets) {
      expect(preset.config, preset.name).toContain(
        "requires_openai_auth = false",
      );
    }
  });

  it("key-based third-party template keeps the fallback flag by default", () => {
    expect(
      generateThirdPartyConfig("acme", "https://api.acme.dev/v1", "m1"),
    ).toContain("requires_openai_auth = true");
  });
});
