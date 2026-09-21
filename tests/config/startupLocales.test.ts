import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

describe("startup locale contract", () => {
  it.each([
    ["en", en],
    ["zh", zh],
    ["zh-TW", zhTw],
    ["ja", ja],
  ])("keeps a branded visible startup message in %s", (_locale, messages) => {
    expect(messages.nav.starting.trim()).not.toBe("");
    expect(messages.nav.starting).toContain(messages.nav.appName);
    expect(messages.nav.localFirst.trim()).not.toBe("");
  });
});
