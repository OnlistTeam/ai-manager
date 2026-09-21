import { CalendarClock } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { useBackupSchedule, useSaveBackupSchedule } from "@/entities/backup";
import { Card } from "@/shared/ui/Card";
import {
  PreferenceSaveStatus,
  type PreferenceSaveState,
} from "@/shared/ui/PreferenceSaveStatus";
import { Switch } from "@/shared/ui/Switch";

/**
 * One sentence says what happens automatically ("once a day, keeps the latest
 * N") and one switch turns it off. The retain count is shown, not edited.
 */
export function BackupScheduleRow() {
  const { t } = useTranslation();
  const schedule = useBackupSchedule();
  const save = useSaveBackupSchedule();
  const controlId = useId();
  const descriptionId = useId();
  const statusId = useId();
  const current = schedule.data;
  const state: PreferenceSaveState = save.isPending
    ? "saving"
    : save.isError
      ? "error"
      : save.isSuccess
        ? "saved"
        : "idle";

  if (current === undefined) return null;

  const label = t("preferences.backup.automatic");
  const description = current.automatic
    ? t("preferences.backup.summaryLine", { count: current.retainCount })
    : t("preferences.backup.automaticOff");

  return (
    <Card padding="none" className="rounded-xl">
      <div className="grid gap-3 p-4 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
        <div className="flex min-w-0 items-start gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
            <CalendarClock className="h-4 w-4" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <label
              htmlFor={controlId}
              className="text-body font-medium text-content"
            >
              {label}
            </label>
            <p
              id={descriptionId}
              className="mt-0.5 text-caption text-content-muted"
            >
              {description}
            </p>
            <PreferenceSaveStatus id={statusId} state={state} />
          </div>
        </div>
        <Switch
          id={controlId}
          checked={current.automatic}
          disabled={save.isPending}
          aria-label={label}
          aria-describedby={
            state === "idle" ? descriptionId : `${descriptionId} ${statusId}`
          }
          onCheckedChange={(checked) =>
            save.mutate({ ...current, automatic: checked })
          }
        />
      </div>
    </Card>
  );
}
