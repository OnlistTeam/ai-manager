import type {
  ProviderConnectionPreset,
  ProviderConnectionProfile,
  ToolLoginAccount,
} from "@/native/schemas/provider";
import type { ToolId } from "@/native/schemas/tool";
import catalog from "../../../../src-tauri/src/compat/ccswitch/provider/provider_presets.generated.json";

/*
 * The add page drawn from the real catalogue, so the gallery shows each tool
 * the cards it actually ships rather than a hand-picked sample. Native reads
 * the address out of each template; this does the same for the four tools the
 * gallery has installed. Development-only: the gallery is never bundled.
 */

interface CatalogEntry {
  tool: string;
  id: string;
  serviceName: string;
  defaultName: string;
  defaultModel: string | null;
  websiteUrl: string;
  apiKeyUrl: string;
  official: boolean;
  default: boolean;
  kind: ProviderConnectionPreset["kind"];
  settingsConfig: {
    env?: Record<string, unknown>;
    config?: unknown;
    options?: { baseURL?: string };
  };
}

const GALLERY_TOOLS: Record<
  string,
  {
    baseUrl: (entry: CatalogEntry) => string;
    modelRequired: boolean;
    baseUrlTakesNoVersion: boolean;
    toolLogin: ToolLoginAccount | null;
  }
> = {
  "claude-code": {
    baseUrl: (entry) => String(entry.settingsConfig.env?.ANTHROPIC_BASE_URL),
    modelRequired: false,
    baseUrlTakesNoVersion: true,
    toolLogin: "claude",
  },
  codex: {
    baseUrl: (entry) =>
      String(entry.settingsConfig.config).match(/base_url = "([^"]+)"/)?.[1] ??
      "",
    modelRequired: false,
    baseUrlTakesNoVersion: false,
    toolLogin: "chatGpt",
  },
  "gemini-cli": {
    baseUrl: (entry) =>
      String(entry.settingsConfig.env?.GOOGLE_GEMINI_BASE_URL),
    modelRequired: false,
    baseUrlTakesNoVersion: true,
    toolLogin: "google",
  },
  opencode: {
    baseUrl: (entry) => entry.settingsConfig.options?.baseURL ?? "",
    modelRequired: true,
    baseUrlTakesNoVersion: false,
    toolLogin: null,
  },
};

export function galleryConnectionProfiles(): Array<
  [ToolId, ProviderConnectionProfile]
> {
  const entries = (catalog as { presets: CatalogEntry[] }).presets;
  return Object.entries(GALLERY_TOOLS).map(([tool, shape]) => {
    const presets = entries.filter((entry) => entry.tool === tool);
    return [
      tool as ToolId,
      {
        defaultPresetId: presets.find((entry) => entry.default)?.id ?? "",
        modelRequired: shape.modelRequired,
        baseUrlTakesNoVersion: shape.baseUrlTakesNoVersion,
        toolLogin: shape.toolLogin,
        presets: presets.map((entry) => ({
          id: entry.id,
          serviceName: entry.serviceName,
          defaultName: entry.defaultName,
          defaultModel: entry.defaultModel,
          baseUrl: shape.baseUrl(entry),
          websiteUrl: entry.websiteUrl,
          apiKeyUrl: entry.apiKeyUrl,
          official: entry.official,
          kind: entry.kind,
        })),
      },
    ];
  });
}
