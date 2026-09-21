import { render, screen } from "@testing-library/react";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zh from "@/i18n/locales/zh.json";
import zhTW from "@/i18n/locales/zh-TW.json";
import { StatusBadge } from "@/shared/ui/StatusBadge";
import { ToolCard } from "@/shared/ui/ToolCard";
import type { Tool } from "@/native/schemas/tool";

/** Collects every leaf key path so the two bundles can be compared. */
function collectKeys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) return [prefix];
  return Object.entries(value as Record<string, unknown>).flatMap(
    ([key, child]) => collectKeys(child, prefix ? `${prefix}.${key}` : key),
  );
}

async function useLocale(language: "en" | "zh"): Promise<void> {
  const bundle = language === "en" ? en : zh;
  i18n.addResourceBundle(
    language,
    "translation",
    { ds: bundle.ds },
    true,
    true,
  );
  await i18n.changeLanguage(language);
}

const TOOL: Tool = {
  id: "claude-code",
  name: "Claude Code",
  descriptionKey: "toolDescriptionFixture",
  status: "notInstalled",
  version: null,
  latestVersion: null,
  capabilities: {
    canInstall: true,
    canUpdate: true,
    canUninstall: true,
    canRepair: true,
    canManageProvider: true,
    canManageMcp: true,
    canManageSkills: true,
    canManagePrompts: true,
    canManageVersion: false,
    canLaunch: true,
  },
  sessionsInsideSettings: false,
  environment: null,
};

describe("ds locale bundle", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("zh");
  });

  it("keeps the English and Chinese trees in sync", () => {
    expect(collectKeys(zh.ds).sort()).toEqual(collectKeys(en.ds).sort());
    expect(collectKeys(en.ds).length).toBeGreaterThan(25);
  });

  it("keeps the ds.* key set identical across every locale", () => {
    const enKeys = collectKeys(en.ds).sort();
    expect(collectKeys(zh.ds).sort()).toEqual(enKeys);
    expect(collectKeys(ja.ds).sort()).toEqual(enKeys);
    expect(collectKeys(zhTW.ds).sort()).toEqual(enKeys);
  });

  it("uses the beginner vocabulary from spec section 57", () => {
    expect(en.ds.action.install).toBe("Install");
    expect(en.ds.action.update).toBe("Update");
    expect(en.ds.action.connect).toBe("Connect");
    expect(en.ds.action.remove).toBe("Remove");
    expect(en.ds.action.check).toBe("Check");
    expect(en.ds.action.fix).toBe("Fix");
    expect(en.ds.action.open).toBe("Open");
    expect(en.ds.status.ready).toBe("Ready");
    const flat = JSON.stringify(en.ds);
    for (const jargon of [
      "Provision",
      "Endpoint",
      "Orchestration",
      "Failover",
    ]) {
      expect(flat).not.toContain(jargon);
    }
  });

  it("renders English copy", async () => {
    await useLocale("en");
    render(<StatusBadge status="attention" />);
    expect(screen.getByText("Needs Attention")).toBeInTheDocument();
  });

  it("renders Chinese copy", async () => {
    await useLocale("zh");
    render(<ToolCard tool={TOOL} onAction={() => {}} />);
    expect(screen.getByText(zh.ds.tool.notInstalled)).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: zh.ds.action.named.install.replace("{{name}}", TOOL.name),
      }),
    ).toBeInTheDocument();
    expect(screen.getByText(zh.ds.action.install)).toBeInTheDocument();
  });
});
