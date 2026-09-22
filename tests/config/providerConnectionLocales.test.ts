import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

describe("provider connection locale contract", () => {
  it.each([
    ["en", en.services],
    ["zh", zh.services],
    ["zh-TW", zhTw.services],
    ["ja", ja.services],
  ])(
    "keeps address-check ownership and result copy complete in %s",
    (_locale, services) => {
      expect(services.action.test.trim()).not.toBe("");
      expect(services.action.testNamed).toContain("{{name}}");
      expect(services.connect.baseUrlHint.trim()).not.toBe("");
      expect(services.connect.baseUrlEdited.trim()).not.toBe("");
      expect(services.connect.afterSave).toContain("{{tool}}");
      expect(services.connect.firstUse).toContain("{{tool}}");
      expect(services.connect.saved).toContain("{{name}}");
      for (const outcome of Object.values(services.connect.address)) {
        expect(outcome).toContain("{{name}}");
      }
      expect(services.connect.addressRetry.trim()).not.toBe("");
      expect(services.connect.speedTest.trim()).not.toBe("");
      expect(services.connect.speedTesting.trim()).not.toBe("");
      expect(services.connect.speedTestAgain.trim()).not.toBe("");
      expect(services.connect.speedTestFailed.trim()).not.toBe("");
      expect(services.connect.speedSorted.trim()).not.toBe("");
      expect(services.connect.measuredServices.trim()).not.toBe("");
      expect(services.connect.responseTime).toContain("{{latency}}");
      expect(services.connect.unreachable.trim()).not.toBe("");
      // The picker names the tool rather than ranking the services: "official"
      // read as a verdict on the provider, and as a limit on what could be
      // used, when it only ever meant "this is the vendor the tool ships with".
      expect(services.connect.toolDefault).toContain("{{tool}}");
      expect(services.connect.toolDefaultGroup).toContain("{{tool}}");
      expect(services.connect.otherServices.trim()).not.toBe("");
      expect(services.connect.presetHint.trim()).not.toBe("");
      expect(services.test.operational.trim()).not.toBe("");
      expect(services.test.degraded.trim()).not.toBe("");
      expect(services.test.failed.trim()).not.toBe("");
    },
  );
});
