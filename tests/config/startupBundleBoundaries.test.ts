import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const ROOT = path.resolve(__dirname, "..", "..");

function source(relativePath: string): string {
  return fs.readFileSync(path.join(ROOT, relativePath), "utf8");
}

describe("renderer startup bundle boundaries", () => {
  it("keeps optional startup recovery dependencies behind dynamic imports", () => {
    const main = source("src/main.tsx");
    const activity = source("src/lib/windowActivity.ts");

    expect(main).not.toMatch(
      /import\s+\{[^}]*message[^}]*\}\s+from\s+"@tauri-apps\/plugin-dialog"/,
    );
    expect(main).not.toMatch(
      /import\s+\{[^}]*exit[^}]*\}\s+from\s+"@tauri-apps\/plugin-process"/,
    );
    expect(main).toContain('import("@tauri-apps/plugin-dialog")');
    expect(main).toContain('import("./components/DatabaseUpgrade")');
    expect(activity).not.toContain(
      'import { getCurrentWindow } from "@tauri-apps/api/window"',
    );
    expect(activity).toContain('import("@tauri-apps/api/window")');
  });

  it("loads the import decision UI only after the prompt is requested", () => {
    const appRoot = source("src/app/AppRoot.tsx");

    expect(appRoot).not.toContain(
      'import { ImportPromptModal } from "@/features/import-existing"',
    );
    expect(appRoot).toContain('import("@/features/import-existing")');
    expect(appRoot).toContain(
      "const [requested, setRequested] = useState(enabled)",
    );
    expect(appRoot).toContain("if (!requested) return null");
  });

  it("keeps silent warming on lightweight capability modules", () => {
    const warming = source("src/app/warmSessionEnvironment.ts");

    expect(warming).not.toContain('from "@/features/extension-management"');
    expect(warming).not.toContain('from "@/features/health"');
    expect(warming).toContain(
      'from "@/features/extension-management/extensionTabs"',
    );
    expect(warming).toContain('from "@/features/health/installedHealthTools"');
  });

  it("keeps framework chunks explicit without a catch-all vendor bucket", () => {
    const config = source("vite.config.ts");

    for (const chunk of [
      "vendor-react",
      "vendor-query",
      "vendor-i18n",
      "vendor-validation",
      "vendor-tauri",
      "vendor-notifications",
      "vendor-ui-core",
    ]) {
      expect(config).toContain(`return "${chunk}"`);
    }
    expect(config).toContain("manualChunks: rendererVendorChunk");
    expect(config).toContain('id.includes("/node_modules/react-i18next/")');
    expect(config).not.toContain('return "vendor"');
  });

  it("packages each locale as an on-demand resource", () => {
    const i18n = source("src/i18n/index.ts");

    expect(i18n).not.toMatch(/import\s+\w+\s+from\s+"\.\/locales\//);
    for (const locale of ["en", "ja", "zh", "zh-TW"]) {
      expect(i18n).toContain(`import("./locales/${locale}.json")`);
    }
    expect(i18n).toContain("export const i18nReady = initializeI18n()");
  });
});
