import { z } from "zod";
import { invokeNative } from "../client";

/**
 * The About panel's Appropriate Legal Notices (AGPL-3.0 §5).
 *
 * The renderer names which notice to open; the native side owns both URLs, so
 * no address ever crosses IPC.
 */
export const about = {
  openSourceCode(): Promise<boolean> {
    return invokeNative("app_about_open_source_code", z.boolean());
  },

  openLicense(): Promise<boolean> {
    return invokeNative("app_about_open_license", z.boolean());
  },
};
