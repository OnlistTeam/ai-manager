import { z } from "zod";
import { invokeNative } from "../client";
import {
  discoverDescriptionsSchema,
  discoverInputValueSchema,
  discoverMcpListSchema,
  discoverSkillListSchema,
  type DiscoverInputValue,
  type DiscoverLink,
  type DiscoverMcpList,
  type DiscoverSkillList,
} from "../schemas/discover";
import type { ExtensionKind, ExtensionScope } from "../schemas/extension";
import { operationIdSchema } from "../schemas/operation";
import type { ToolId } from "../schemas/tool";

export interface DiscoverMcpInstall {
  server: string;
  values: DiscoverInputValue[];
  /** The translated one-liner, kept as the new connection's description. */
  description: string | null;
  scopes: ExtensionScope[];
}

/**
 * The Discover section of the Skills and MCP pages (ADR-0063). Native owns
 * every address: the renderer sends search words, ids and the apps to add
 * to, never a URL to open or a command to run.
 */
export const discover = {
  mcpList(query: string): Promise<DiscoverMcpList> {
    return invokeNative("app_discover_mcp_list", discoverMcpListSchema, {
      query,
    });
  },

  skillList(query: string): Promise<DiscoverSkillList> {
    return invokeNative("app_discover_skill_list", discoverSkillListSchema, {
      query,
    });
  },

  skillDescriptions(ids: readonly string[]): Promise<Record<string, string>> {
    return invokeNative(
      "app_discover_skill_descriptions",
      discoverDescriptionsSchema,
      { ids: [...ids] },
    );
  },

  /** A picture the section handed out, as a `data:` URL. */
  icon(url: string): Promise<string> {
    return invokeNative(
      "app_discover_icon",
      z.string().startsWith("data:image/"),
      { url },
    );
  },

  installMcp(install: DiscoverMcpInstall): Promise<string> {
    return invokeNative("app_discover_mcp_install", operationIdSchema, {
      server: install.server,
      values: z.array(discoverInputValueSchema).parse(install.values),
      description: install.description,
      scopes: install.scopes,
    });
  },

  installSkill(skill: string, tools: readonly ToolId[]): Promise<string> {
    return invokeNative("app_discover_skill_install", operationIdSchema, {
      skill,
      tools: [...tools],
    });
  },

  openLink(kind: ExtensionKind, id: string, link: DiscoverLink): Promise<null> {
    return invokeNative("app_discover_link_open", z.null(), {
      kind,
      id,
      link,
    });
  },
};
