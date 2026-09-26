import { invokeNative } from "../client";
import {
  privacyProtectionSchema,
  type PrivacyProtection,
} from "../schemas/privacyProtection";

export const privacyProtection = {
  get(): Promise<PrivacyProtection> {
    return invokeNative("app_privacy_protection_get", privacyProtectionSchema);
  },

  /** Returns the value read back after saving. */
  set(enabled: boolean): Promise<PrivacyProtection> {
    return invokeNative("app_privacy_protection_set", privacyProtectionSchema, {
      enabled,
    });
  },
};
