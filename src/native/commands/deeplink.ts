import { z } from "zod";
import { invokeNative } from "../client";
import {
  deepLinkImportOutcomeSchema,
  deepLinkPreviewListSchema,
  deepLinkPreviewSchema,
  type DeepLinkImportOutcome,
  type DeepLinkPreview,
} from "../schemas/deeplink";

/**
 * One-click import (ADR-0029). The queue lives in native memory: the renderer
 * only ever holds opaque pending ids, and a link it did not paste itself never
 * crosses this boundary in either direction.
 */
export const deepLink = {
  pending(): Promise<DeepLinkPreview[]> {
    return invokeNative("app_deeplink_pending_list", deepLinkPreviewListSchema);
  },

  preview(pending: string): Promise<DeepLinkPreview> {
    return invokeNative("app_deeplink_preview", deepLinkPreviewSchema, {
      pending,
    });
  },

  /** The paste path may carry a credential; the registered-scheme path may not. */
  submitPasted(link: string): Promise<DeepLinkPreview> {
    return invokeNative("app_deeplink_submit_pasted", deepLinkPreviewSchema, {
      link,
    });
  },

  dismiss(pending: string): Promise<void> {
    return invokeNative("app_deeplink_dismiss", z.void(), { pending });
  },

  confirm(pending: string): Promise<DeepLinkImportOutcome> {
    return invokeNative("app_deeplink_confirm", deepLinkImportOutcomeSchema, {
      pending,
    });
  },
};
