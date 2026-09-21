import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

describe("home guidance locale contract", () => {
  it.each([
    ["en", en],
    ["zh", zh],
    ["zh-TW", zhTw],
    ["ja", ja],
  ])("keeps the complete next-step copy in %s", (_locale, messages) => {
    expect(messages.home.card.startTool.trim()).not.toBe("");
    expect(messages.home.card.startHint.trim()).not.toBe("");
    expect(messages.home.card.inventory.label.trim()).not.toBe("");
    expect(messages.home.card.inventory.servicesConfigured_one.trim()).not.toBe(
      "",
    );
    expect(
      messages.home.card.inventory.servicesConfigured_other.trim(),
    ).not.toBe("");
    expect(messages.home.card.inventory.mcpEnabled.trim()).not.toBe("");
    expect(messages.home.card.inventory.noMcp.trim()).not.toBe("");
    expect(messages.home.card.actions).toEqual({
      tools: expect.any(String),
      services: expect.any(String),
      extensions: expect.any(String),
    });
    expect(Object.values(messages.home.card.actions)).not.toContain("");
  });
});
