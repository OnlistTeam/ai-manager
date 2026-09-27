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
      expect(services.connect.customBaseUrlHint.trim()).not.toBe("");
      expect(services.connect.keyOptionalHint.trim()).not.toBe("");
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
      expect(services.connect.responseTime).toContain("{{latency}}");
      expect(services.connect.unreachable.trim()).not.toBe("");
      // The add page (ADR-0057): the breadcrumb and the subscription card
      // name the tool, and every group has a heading and a hint.
      expect(services.add.title).toContain("{{tool}}");
      expect(services.add.loginDetail).toContain("{{tool}}");
      expect(services.add.noMatch).toContain("{{query}}");
      for (const kind of ["login", "vendor", "relay", "local"] as const) {
        expect(services.add.group[kind].trim()).not.toBe("");
        expect(services.add.groupHint[kind].trim()).not.toBe("");
      }
      expect(services.add.groupHint.login).toContain("{{tool}}");
      // Signing in from AI Manager (ADR-0061): every way it can fail is
      // said, and the ones about the tool name it.
      expect(services.login.useToolLogin).toContain("{{tool}}");
      expect(services.login.failure.notInstalled).toContain("{{tool}}");
      for (const failure of Object.values(services.login.failure)) {
        expect(failure.trim()).not.toBe("");
      }
      expect(services.test.operational.trim()).not.toBe("");
      expect(services.test.degraded.trim()).not.toBe("");
      expect(services.test.failed.trim()).not.toBe("");
    },
  );
});
