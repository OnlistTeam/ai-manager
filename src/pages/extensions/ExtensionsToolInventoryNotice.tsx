import { Puzzle } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { EmptyState } from "@/shared/ui/EmptyState";
import { InventoryRefreshNotice } from "@/shared/ui/InventoryRefreshNotice";

interface ExtensionsToolsUnavailableProps {
  refreshing: boolean;
  retryButtonRef: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Keeps the initial failure and its keyboard context mounted during retry. */
export function ExtensionsToolsUnavailable({
  refreshing,
  retryButtonRef,
  onRetry,
}: ExtensionsToolsUnavailableProps) {
  const { t } = useTranslation();

  return (
    <div
      role="alert"
      aria-label={t("extensions.toolsError.title")}
      aria-busy={refreshing || undefined}
    >
      <EmptyState
        icon={Puzzle}
        title={t("extensions.toolsError.title")}
        description={t("extensions.toolsError.description")}
        action={
          <Button ref={retryButtonRef} loading={refreshing} onClick={onRetry}>
            {t("extensions.refresh")}
          </Button>
        }
      />
    </div>
  );
}

interface ExtensionsToolRefreshNoticeProps {
  refreshing: boolean;
  retryButtonRef: Ref<HTMLButtonElement>;
  onRetry: () => void;
}

/** Explains why retained extension content is temporarily read-only. */
export function ExtensionsToolRefreshNotice({
  refreshing,
  retryButtonRef,
  onRetry,
}: ExtensionsToolRefreshNoticeProps) {
  const { t } = useTranslation();

  return (
    <InventoryRefreshNotice
      title={t("extensions.toolsRefreshError.title")}
      description={t("extensions.toolsRefreshError.description")}
      actionLabel={t("extensions.toolsRefreshError.action")}
      refreshing={refreshing}
      retryButtonRef={retryButtonRef}
      onRetry={onRetry}
    />
  );
}
