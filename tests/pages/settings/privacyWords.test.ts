import { describe, expect, it } from "vitest";
import {
  formatPrivacyWords,
  parsePrivacyWords,
  sameWords,
} from "@/pages/settings/privacyWords";

describe("privacy words", () => {
  it("splits at ASCII, fullwidth and ideographic commas and line breaks", () => {
    expect(
      parsePrivacyWords("acme, Project Kite，张三、李四\nhost-01\r\nx, , acme"),
    ).toEqual(["acme", "Project Kite", "张三", "李四", "host-01"]);
  });

  it("drops words under two characters, counting characters not bytes", () => {
    expect(parsePrivacyWords("a, 张, 张三, ab")).toEqual(["张三", "ab"]);
    expect(parsePrivacyWords("   ")).toEqual([]);
  });

  it("formats words the way the field shows them", () => {
    expect(formatPrivacyWords(["acme", "张三"])).toBe("acme, 张三");
    expect(formatPrivacyWords([])).toBe("");
  });

  it("compares lists in order", () => {
    expect(sameWords(["a1", "b2"], ["a1", "b2"])).toBe(true);
    expect(sameWords(["a1", "b2"], ["b2", "a1"])).toBe(false);
    expect(sameWords(["a1"], ["a1", "b2"])).toBe(false);
  });
});
