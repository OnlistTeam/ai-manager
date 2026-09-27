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

/** Lines the notice up under the tool name, past the glyph. */
const NOTICE_CLASS = "flex min-w-0 items-start gap-1.5 pl-10 text-caption";

/**
 * What the last switch from this row left behind, as a small line under it.
 * Nothing here needs its own retry: choosing the endpoint again in the row's
 * picker is the retry, and the fuller recovery (trying the next endpoint,
 * browsing compatible ones) lives on the API Endpoints page, where the failed
 * check is already recorded.
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
      <div role="alert" aria-label={title} className={NOTICE_CLASS}>
        <AlertCircle
          className="mt-0.5 h-3.5 w-3.5 shrink-0 text-danger"
          aria-hidden="true"
        />
        <div className="min-w-0 leading-5">
          <p className="text-content">
            <span className="font-medium">{title}</span>{" "}
            <span className="text-content-muted">{t(copy.messageKey)}</span>
          </p>
          <ErrorDetailsDisclosure copy={copy} />
        </div>
      </div>
    );
  }

  if (unreachableName) {
    return (
      <div role="status" className={`${NOTICE_CLASS} items-center`}>
        <Radio
          className="h-3.5 w-3.5 shrink-0 text-warning"
          aria-hidden="true"
        />
        <span className="min-w-0 text-content">
          {t("home.tools.switchUnreachable", { name: unreachableName })}
        </span>
        <Button
          variant="ghost"
          size="xs"
          className="-my-1 h-6"
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
