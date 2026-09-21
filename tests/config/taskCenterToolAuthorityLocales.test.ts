import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

const locales = { en, ja, "zh-TW": zhTw, zh } as const;

describe("Task Center tool-authority locale contract", () => {
  for (const [locale, messages] of Object.entries(locales)) {
    it(`${locale} keeps every recovery and paused-action message`, () => {
      const authority = (
        messages.taskCenter as typeof messages.taskCenter & {
          toolAuthority?: Record<string, string>;
        }
      ).toolAuthority;

      expect(authority).toEqual({
        openLabel: expect.any(String),
        unavailableTitle: expect.any(String),
        unavailableDescription: expect.any(String),
        refreshTitle: expect.any(String),
        refreshDescription: expect.any(String),
        retry: expect.any(String),
        pausedTitle: expect.any(String),
        pausedDescription: expect.any(String),
        changedTitle: expect.any(String),
        changedDescription: expect.any(String),
      });
      expect(Object.values(authority ?? {})).not.toContain("");
    });
  }
});
