import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

describe("AI Services authority recovery locale contract", () => {
  it.each([
    ["en", en],
    ["zh", zh],
    ["zh-TW", zhTw],
    ["ja", ja],
  ])(
    "keeps stale-safe service recovery complete in %s",
    (_locale, messages) => {
      const services = messages.services as typeof messages.services & {
        toolsRefreshError?: Record<string, string>;
        actionsPaused?: Record<string, string>;
      };

      expect(services.toolsRefreshError).toEqual({
        title: expect.any(String),
        description: expect.any(String),
        action: expect.any(String),
      });
      expect(services.actionsPaused).toEqual({
        title: expect.any(String),
        description: expect.any(String),
      });
      expect(Object.values(services.toolsRefreshError ?? {})).not.toContain("");
      expect(Object.values(services.actionsPaused ?? {})).not.toContain("");
    },
  );
});
