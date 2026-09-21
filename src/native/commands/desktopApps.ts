import { invokeNative } from "../client";
import {
  desktopAppLaunchOutcomeSchema,
  desktopAppListSchema,
  desktopAppOfficialDownloadOutcomeSchema,
  desktopAppUninstallOutcomeSchema,
  type DesktopApp,
  type DesktopAppId,
  type DesktopAppLaunchOutcome,
  type DesktopAppOfficialDownloadOutcome,
  type DesktopAppUninstallOutcome,
} from "../schemas/desktopApp";

export const desktopApps = {
  list(): Promise<DesktopApp[]> {
    return invokeNative("app_desktop_apps_list", desktopAppListSchema);
  },

  /** Native resolves the current fixed bundle/package identity; no path crosses IPC. */
  launch(app: DesktopAppId): Promise<DesktopAppLaunchOutcome> {
    return invokeNative(
      "app_desktop_app_launch",
      desktopAppLaunchOutcomeSchema,
      { app },
    );
  },

  /** Opens a native-owned vendor URL; the renderer never supplies a URL. */
  openOfficialDownload(
    app: DesktopAppId,
  ): Promise<DesktopAppOfficialDownloadOutcome> {
    return invokeNative(
      "app_desktop_app_open_official_download",
      desktopAppOfficialDownloadOutcomeSchema,
      { app },
    );
  },

  /** Opens a native-owned OS uninstall handoff; renderer never supplies a path or package id. */
  openUninstall(app: DesktopAppId): Promise<DesktopAppUninstallOutcome> {
    return invokeNative(
      "app_desktop_app_open_uninstall",
      desktopAppUninstallOutcomeSchema,
      { app },
    );
  },
};
