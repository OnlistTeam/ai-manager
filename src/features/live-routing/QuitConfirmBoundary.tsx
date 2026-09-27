import { useTranslation } from "react-i18next";
import {
  useConfirmQuit,
  useQuitRequest,
  type RoutedTool,
} from "@/entities/routing";
import { ConfirmActionModal } from "@/features/tool-management";

function names(
  tools: RoutedTool[],
  pickup: RoutedTool["pickup"],
  t: (key: string) => string,
): string {
  return tools
    .filter((routed) => routed.pickup === pickup)
    .map((routed) => t(`routing.tool.${routed.tool}`))
    .join(t("routing.live.listSeparator"));
}

/**
 * Asks before AI Manager quits while tools are routed through it (ADR-0054):
 * which tools go back to their own settings, and which of their open
 * sessions need a restart. Mounted beside the shell so the question reaches
 * the user from any page.
 */
export function QuitConfirmBoundary() {
  const { t } = useTranslation();
  const request = useQuitRequest();
  const confirm = useConfirmQuit();
  const tools = request.tools;
  if (!tools) return null;

  const all = tools
    .map((routed) => t(`routing.tool.${routed.tool}`))
    .join(t("routing.live.listSeparator"));
  const live = names(tools, "live", t);
  const atStart = names(tools, "atStart", t);

  return (
    <ConfirmActionModal
      open
      onOpenChange={(open) => {
        if (open || confirm.isPending) return;
        confirm.reset();
        request.dismiss();
      }}
      title={t("routing.quit.title")}
      description={t("routing.quit.description", { tools: all })}
      confirmLabel={t("routing.quit.confirm")}
      confirmTone="danger"
      busy={confirm.isPending}
      error={confirm.error}
      onConfirm={() => confirm.mutate()}
    >
      <ul className="flex list-disc flex-col gap-1 pl-5 text-caption text-content-muted">
        {live ? (
          <li data-quit-pickup="live">
            {t("routing.quit.live", { tools: live })}
          </li>
        ) : null}
        {atStart ? (
          <li data-quit-pickup="atStart">
            {t("routing.quit.atStart", { tools: atStart })}
          </li>
        ) : null}
      </ul>
    </ConfirmActionModal>
  );
}
