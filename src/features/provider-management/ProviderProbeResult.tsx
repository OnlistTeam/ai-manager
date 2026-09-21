import { AlertTriangle, CheckCircle2, CircleSlash, Link2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ModelProbeOutcome } from "@/native";
import { Badge } from "@/shared/ui/Badge";

interface ProviderProbeResultProps {
  outcome: ModelProbeOutcome;
}

/**
 * What the service actually answered.
 *
 * A refusal is rendered here rather than as an error card: the probe did its
 * job, and "HTTP 401, invalid api key" is the answer the user came for.
 */
export function ProviderProbeResult({ outcome }: ProviderProbeResultProps) {
  const { t } = useTranslation();
  const reply = outcome.reply;
  const answered = reply.kind === "text" || reply.kind === "image";

  return (
    <section
      aria-label={t("services.probe.result")}
      className="flex flex-col gap-2 border-t border-hairline pt-4"
    >
      <div className="flex items-center gap-2">
        <Badge
          tone={answered ? "success" : "warning"}
          icon={answered ? CheckCircle2 : AlertTriangle}
        >
          {t("services.probe.resultMeta", {
            status: outcome.httpStatus ?? "—",
            ms: outcome.latencyMs,
          })}
        </Badge>
        <span className="truncate font-mono text-caption text-content-muted">
          {outcome.model}
        </span>
      </div>

      {reply.kind === "text" ? (
        <p className="max-h-64 overflow-auto whitespace-pre-wrap rounded-xl bg-layer-1 p-3 text-body leading-6 text-content">
          {reply.text}
        </p>
      ) : null}

      {reply.kind === "image" ? (
        <img
          src={`data:${reply.mime};base64,${reply.base64}`}
          alt={t("services.probe.imageAlt", { model: outcome.model })}
          className="max-h-80 w-auto self-start rounded-xl border border-hairline"
        />
      ) : null}

      {reply.kind === "rejected" ? (
        <div className="flex flex-col gap-1.5 rounded-xl border border-warning/20 bg-warning/[0.06] p-3">
          <p className="text-caption leading-5 text-content">
            {t("services.probe.replyRefused")}
          </p>
          {reply.detail === "" ? null : (
            <p className="max-h-40 overflow-auto whitespace-pre-wrap font-mono text-caption leading-5 text-content-muted">
              {reply.detail}
            </p>
          )}
        </div>
      ) : null}

      {reply.kind === "empty" ? (
        <p className="flex items-start gap-2 text-caption leading-5 text-content-muted">
          <CircleSlash className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
          {t("services.probe.replyEmpty")}
        </p>
      ) : null}

      {reply.kind === "imageLinkOnly" ? (
        <p className="flex items-start gap-2 text-caption leading-5 text-content-muted">
          <Link2 className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
          {t("services.probe.replyImageLinkOnly")}
        </p>
      ) : null}
    </section>
  );
}
