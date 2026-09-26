import { invokeNative } from "../client";
import {
  privacyProtectionSchema,
  type PrivacyProtection,
  type PrivacyProtectionPatch,
} from "../schemas/privacyProtection";

export const privacyProtection = {
  get(): Promise<PrivacyProtection> {
    return invokeNative("app_privacy_protection_get", privacyProtectionSchema);
  },

  /** Applies `patch` and returns every setting as read back after saving. */
  set(patch: PrivacyProtectionPatch): Promise<PrivacyProtection> {
    return invokeNative("app_privacy_protection_set", privacyProtectionSchema, {
      patch,
    });
  },
};
