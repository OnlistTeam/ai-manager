import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

const LOCALES = [
  ["en", en],
  ["zh", zh],
  ["zh-TW", zhTw],
  ["ja", ja],
] as const;

describe("home guidance locale contract", () => {
  it.each(LOCALES)(
    "keeps the complete next-step copy in %s",
    (_locale, messages) => {
      expect(messages.home.status.startTool).toContain("{{name}}");
      expect(messages.home.status.actions).toEqual({
        tools: expect.any(String),
        services: expect.any(String),
        mcp: expect.any(String),
      });
      expect(Object.values(messages.home.status.actions)).not.toContain("");
    },
  );

  it.each(LOCALES)(
    "names every connection state a tool row can show in %s",
    (_locale, messages) => {
      const tools = messages.home.tools;
      for (const value of [
        tools.official,
        tools.notConnected,
        tools.unavailable,
        tools.modelInTool,
        tools.manageEndpoints,
        tools.addEndpoint,
        tools.noEndpoints,
      ]) {
        expect(value.trim()).not.toBe("");
      }
      expect(tools.added_other).toContain("{{count}}");
      expect(tools.switchUnreachable).toContain("{{name}}");
      expect(tools.pickNamed).toContain("{{tool}}");
      expect(tools.pickNamed).toContain("{{current}}");
    },
  );
});
