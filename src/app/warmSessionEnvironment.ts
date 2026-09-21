import type { QueryClient } from "@tanstack/react-query";
import { backupsQueryOptions } from "@/entities/backup";
import { desktopAppsQueryOptions } from "@/entities/desktop-app";
import {
  extensionsQueryOptions,
  localExtensionInventoryQueryOptions,
} from "@/entities/extension";
import { healthSnapshotQueryOptions } from "@/entities/health";
import { importPreviewQueryOptions } from "@/entities/import";
import { networkProxyQueryOptions } from "@/entities/network-proxy";
import {
  providerConnectionProfileQueryOptions,
  providerRuntimeContextQueryOptions,
  providersQueryOptions,
} from "@/entities/provider";
import { sessionsQueryOptions } from "@/entities/session";
import { productSettingsQueryOptions } from "@/entities/settings";
import { skillUpdatesQueryOptions } from "@/entities/skill-update";
import { skillBackupsQueryOptions } from "@/entities/skill-backup";
import { skillRepositoriesQueryOptions } from "@/entities/skill-repository";
import { toolsQueryOptions } from "@/entities/tool";
import { supportedExtensionScopes } from "@/features/extension-management/extensionTabs";
import { installedHealthTools } from "@/features/health/installedHealthTools";

/**
 * Starts one silent environment read for the process. TanStack deduplicates
 * any page that mounts while this is running, then the session cache serves
 * every later navigation until a manual refresh or mutation invalidation.
 * Failures stay in their normal Query state; startup itself never blocks.
 */
export async function warmSessionEnvironment(
  queryClient: QueryClient,
): Promise<void> {
  // These independent scans begin immediately, but a slow desktop-app or
  // extension discovery cannot delay the scope-specific follow-up reads.
  const independentWarm = Promise.allSettled([
    queryClient.ensureQueryData(backupsQueryOptions()),
    queryClient.ensureQueryData(localExtensionInventoryQueryOptions()),
    queryClient.ensureQueryData(networkProxyQueryOptions()),
    queryClient.ensureQueryData(skillUpdatesQueryOptions()),
    queryClient.ensureQueryData(skillBackupsQueryOptions()),
    queryClient.ensureQueryData(skillRepositoriesQueryOptions()),
    // Local Data promoted sessions to a first-visit destination; warm the
    // unfiltered list the page mounts with (SessionsPage.tsx's initial
    // `useState`) so opening it never starts a fresh disk scan.
    queryClient.ensureQueryData(sessionsQueryOptions("", null)),
  ]);
  const [toolsResult, settingsResult, desktopAppsResult] =
    await Promise.allSettled([
      queryClient.ensureQueryData(toolsQueryOptions()),
      queryClient.ensureQueryData(productSettingsQueryOptions()),
      queryClient.ensureQueryData(desktopAppsQueryOptions()),
    ]);

  const followUpWarm: Promise<unknown>[] = [independentWarm];

  if (
    settingsResult.status === "fulfilled" &&
    !settingsResult.value.importPromptSeen
  ) {
    followUpWarm.push(queryClient.ensureQueryData(importPreviewQueryOptions()));
  }

  if (toolsResult.status !== "fulfilled") {
    await Promise.allSettled(followUpWarm);
    return;
  }

  const manageable = toolsResult.value.filter(
    (tool) => tool.capabilities.canManageProvider,
  );
  const extensionScopes = supportedExtensionScopes(
    toolsResult.value,
    desktopAppsResult.status === "fulfilled" ? desktopAppsResult.value : [],
  );

  followUpWarm.push(
    queryClient.ensureQueryData(
      healthSnapshotQueryOptions(installedHealthTools(toolsResult.value)),
    ),
    // A service scope is a local configuration surface, not a remote feed.
    // Warm every supported scope once so changing the service tab later never
    // turns into another several-second first read. Query deduplication keeps
    // an already-mounted page on the same request while this runs.
    ...manageable.flatMap((tool) => [
      queryClient.ensureQueryData(providersQueryOptions(tool.id)),
      queryClient.ensureQueryData(
        providerConnectionProfileQueryOptions(tool.id),
      ),
      queryClient.ensureQueryData(providerRuntimeContextQueryOptions(tool.id)),
    ]),
    ...extensionScopes.map(({ scope, kind }) =>
      queryClient.ensureQueryData(extensionsQueryOptions(scope, kind)),
    ),
  );

  await Promise.allSettled(followUpWarm);
}
