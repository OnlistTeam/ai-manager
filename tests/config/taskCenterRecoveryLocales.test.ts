import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

describe("Task Center recovery locale contract", () => {
  it.each([
    ["en", en],
    ["zh", zh],
    ["zh-TW", zhTw],
    ["ja", ja],
  ])(
    "keeps the retained-history warning complete in %s",
    (_locale, messages) => {
      expect(messages.taskCenter.openUnavailable.trim()).not.toBe("");
      expect(messages.taskCenter.openRefreshError.trim()).not.toBe("");
      expect(messages.taskCenter.refreshError).toEqual({
        summary: expect.any(String),
        title: expect.any(String),
        description: expect.any(String),
        retry: expect.any(String),
      });
      expect(Object.values(messages.taskCenter.refreshError)).not.toContain("");
    },
  );
});
