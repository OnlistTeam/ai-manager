import { Archive, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";

export interface FirstBackupCardProps {
  creating: boolean;
  disabled?: boolean;
  onCreate: () => void;
}

export function FirstBackupCard({
  creating,
  disabled = false,
  onCreate,
}: FirstBackupCardProps) {
  const { t } = useTranslation();
  const title = t("preferences.backup.empty.title");

  return (
    <Card
      role="status"
      aria-label={title}
      padding="none"
      className="overflow-hidden border-brand/15 bg-layer-1 shadow-sm"
    >
      <div className="flex flex-col gap-4 p-5 sm:flex-row sm:items-center">
        <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border border-brand/15 bg-brand/10 text-brand shadow-sm">
          <ShieldCheck className="h-5 w-5" aria-hidden="true" />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-body font-medium text-content">{title}</p>
            <Badge tone="success">{t("preferences.backup.empty.local")}</Badge>
          </div>
          <p className="mt-1 text-caption leading-5 text-content-muted">
            {t("preferences.backup.empty.description")}
          </p>
        </div>
        <Button
          loading={creating}
          disabled={disabled}
          className="self-stretch sm:self-auto"
          onClick={onCreate}
        >
          {!creating ? (
            <Archive className="h-4 w-4" aria-hidden="true" />
          ) : null}
          {t("preferences.backup.create")}
        </Button>
      </div>
    </Card>
  );
}
