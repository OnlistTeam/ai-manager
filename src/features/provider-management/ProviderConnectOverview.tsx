import {
  CircleDollarSign,
  ExternalLink,
  KeyRound,
  RadioTower,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ProviderConnectionPreset } from "@/entities/provider";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { cn } from "@/shared/ui/cn";

interface ProviderConnectOverviewProps {
  preset: ProviderConnectionPreset;
  toolName: string;
}

export function ProviderConnectOverview({
  preset,
  toolName,
}: ProviderConnectOverviewProps) {
  const { t } = useTranslation();
  return (
    <>
      <div className="flex items-center gap-3 rounded-lg border border-hairline bg-layer-1 px-4 py-2.5">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-brand/10 text-brand">
          <KeyRound className="h-4 w-4" aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate text-body font-medium text-content">
            {preset.serviceName}
          </p>
          <p className="truncate text-caption text-content-muted">
            {t("services.connect.forTool", { tool: toolName })}
          </p>
        </div>
        <a
          href={preset.apiKeyUrl}
          target="_blank"
          rel="noreferrer"
          className={cn(
            "inline-flex shrink-0 items-center gap-1 rounded-sm text-caption font-medium text-brand hover:text-brand-hover",
            FOCUS_RING,
          )}
        >
          {t(
            preset.official
              ? "services.connect.getKey"
              : "services.connect.openProvider",
          )}
          <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
        </a>
      </div>

      <aside className="grid gap-1.5 rounded-lg border border-brand/15 bg-brand/[0.04] px-4 py-2.5 text-caption leading-5 text-content-muted">
        <p className="flex items-start gap-2">
          <CircleDollarSign
            className="mt-0.5 h-4 w-4 shrink-0 text-brand"
            aria-hidden="true"
          />
          <span>
            {t("services.connect.accountHint", {
              service: preset.serviceName,
            })}
          </span>
        </p>
        <p className="flex items-start gap-2">
          <RadioTower
            className="mt-0.5 h-4 w-4 shrink-0 text-brand"
            aria-hidden="true"
          />
          <span>{t("services.connect.afterSave", { tool: toolName })}</span>
        </p>
      </aside>
    </>
  );
}
