import { z } from "zod";
import { invokeNative } from "../client";
import {
  initErrorPayloadSchema,
  type InitErrorPayload,
} from "../schemas/system";

export const system = {
  /** The startup failure the backend recorded before the window opened, if any. */
  initError(): Promise<InitErrorPayload | null> {
    return invokeNative("get_init_error", initErrorPayloadSchema.nullable());
  },

  /**
   * Tells the native chrome (Windows title bar, macOS traffic lights) which
   * appearance to use. The product has a single coloured look, so this is set
   * once at boot rather than tracked as a preference.
   */
  setWindowTheme(theme: "dark" | "light" | "system"): Promise<void> {
    return invokeNative("set_window_theme", z.void(), { theme });
  },

  /**
   * Shows a path in Finder / Explorer. The native side falls back to the
   * nearest existing folder when the path itself has gone away.
   */
  revealPath(path: string): Promise<void> {
    return invokeNative("app_reveal_path", z.void(), {
      path: z.string().min(1).parse(path),
    });
  },
};
