import { AlertCircle, ChevronRight, Radio } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { ErrorDetailsDisclosure } from "@/shared/ui/ErrorDetailsDisclosure";
import type { HomeToolConnection } from "./useHomeToolConnection";

interface HomeToolRowNoticeProps {
  connection: HomeToolConnection;
  onOpenServices: () => void;
}

/**
 * What the last switch from this row left behind. Nothing here needs its own
 * retry: choosing the endpoint again in the row's menu is the retry, and the
 * fuller recovery (trying the next endpoint, browsing compatible ones) lives
 * on the API Endpoints page, where the failed check is already recorded.
 */
export function HomeToolRowNotice({
  connection,
  onOpenServices,
}: HomeToolRowNoticeProps) {
  const { t } = useTranslation();
  const { failure, unreachableName } = connection;

  if (failure) {
    const copy = toErrorCopy(failure.error);
    const title = t("services.switch.errorNamed", {
      name: failure.name ?? t("services.failover.unknownService"),
    });
    return (
      <div
        role="alert"
        aria-label={title}
        className="flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 px-3 py-2"
      >
        <AlertCircle
          className="mt-0.5 h-4 w-4 shrink-0 text-danger"
          aria-hidden="true"
        />
        <div className="min-w-0 text-caption leading-5">
          <p className="font-medium text-content">{title}</p>
          <p className="text-content-muted">{t(copy.messageKey)}</p>
          <ErrorDetailsDisclosure copy={copy} />
        </div>
      </div>
    );
  }

  if (unreachableName) {
    return (
      <div
        role="status"
        className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-warning/30 bg-warning/5 px-3 py-1.5 text-caption"
      >
        <Radio className="h-4 w-4 shrink-0 text-warning" aria-hidden="true" />
        <span className="min-w-0 flex-1 text-content">
          {t("home.tools.switchUnreachable", { name: unreachableName })}
        </span>
        <Button
          variant="ghost"
          size="xs"
          className="-my-1"
          onClick={onOpenServices}
        >
          {t("home.status.actions.services")}
          <ChevronRight className="h-3.5 w-3.5" aria-hidden="true" />
        </Button>
      </div>
    );
  }

  return null;
}
