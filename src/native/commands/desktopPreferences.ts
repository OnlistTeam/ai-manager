import { invokeNative } from "../client";
import {
  desktopPreferencesSchema,
  type DesktopPreferences,
} from "../schemas/desktopPreferences";

export const desktopPreferences = {
  get(): Promise<DesktopPreferences> {
    return invokeNative(
      "app_desktop_preferences_get",
      desktopPreferencesSchema,
    );
  },

  save(settings: DesktopPreferences): Promise<DesktopPreferences> {
    return invokeNative(
      "app_desktop_preferences_save",
      desktopPreferencesSchema,
      { settings },
    );
  },
};
