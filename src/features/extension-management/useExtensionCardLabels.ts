import { useTranslation } from "react-i18next";
import type { Extension } from "@/entities/extension";
import type { ExtensionToggleFailure } from "./ExtensionCard";

export interface ExtensionCardLabels {
  item: string;
  use: string;
  remove: string;
  edit: string;
  update: string;
  openLocation: string;
  editDetected: string;
  copyTo: string;
  /** Names the retry for a failed toggle; null when nothing failed. */
  retry: string | null;
  /** Heading of the inline error; null when there is nothing to report. */
  failureTitle: string | null;
  /** Which tools a detected extension also lives in. */
  presence: string;
}

/**
 * Every accessible name the card needs, computed once. Names carry the
 * extension's name and list position so same-named entries stay apart for
 * assistive technology.
 */
export function useExtensionCardLabels(
  extension: Extension,
  position: number,
  total: number,
  failure: ExtensionToggleFailure | undefined,
  detectedResourceError: Error | undefined,
  presentIn: readonly string[] | undefined,
): ExtensionCardLabels {
  const { t } = useTranslation();
  const named = { name: extension.name, position, total };

  return {
    item: t("extensions.card.itemLabel", named),
    use: t("extensions.card.useNamed", named),
    remove: t("extensions.card.removeNamed", named),
    edit: t("extensions.card.editNamed", named),
    update: t("extensions.card.updateNamed", named),
    openLocation: t("extensions.card.openLocationNamed", {
      name: extension.name,
    }),
    editDetected: t("extensions.card.editDocumentNamed", {
      name: extension.name,
    }),
    copyTo: t("extensions.card.copyToNamed", { name: extension.name }),
    retry: failure
      ? t(
          failure.intendedEnabled
            ? "extensions.card.retryEnableNamed"
            : "extensions.card.retryDisableNamed",
          named,
        )
      : null,
    failureTitle: failure
      ? t(
          failure.intendedEnabled
            ? "extensions.card.enableErrorNamed"
            : "extensions.card.disableErrorNamed",
          { name: extension.name },
        )
      : detectedResourceError
        ? t("extensions.card.resourceOpenErrorNamed", {
            name: extension.name,
          })
        : null,
    presence:
      presentIn === undefined || presentIn.length === 0
        ? t("extensions.card.detectedDescription")
        : presentIn.length === 1
          ? t("extensions.card.onlyIn", { tool: presentIn[0] })
          : t("extensions.card.alsoIn", { tools: presentIn }),
  };
}
