import { EyeOff, Minimize2, PanelTop, Rocket } from "lucide-react";
import { useId, useState, type ComponentType } from "react";
import { useTranslation } from "react-i18next";
import {
  useDesktopPreferences,
  useSaveDesktopPreferences,
  type DesktopPreferences,
} from "@/entities/desktop-preferences";
import { isMac } from "@/lib/platform";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import { Switch } from "@/shared/ui/Switch";
import {
  PreferenceSaveStatus,
  type PreferenceSaveState,
} from "@/shared/ui/PreferenceSaveStatus";

type PreferenceName = keyof DesktopPreferences;

function DesktopPreferenceRow({
  icon: Icon,
  label,
  description,
  checked,
  disabled,
  saveState,
  onChange,
}: {
  icon: ComponentType<{ className?: string }>;
  label: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  saveState: PreferenceSaveState;
  onChange: (checked: boolean) => void;
}) {
  const controlId = useId();
  const descriptionId = useId();
  const statusId = useId();
  const describedBy =
    saveState === "idle" ? descriptionId : `${descriptionId} ${statusId}`;

  return (
    <div className="grid gap-3 border-b border-hairline py-4 last:border-b-0 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <div className="flex min-w-0 items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
          <Icon className="h-4 w-4" aria-hidden="true" />
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
          <PreferenceSaveStatus id={statusId} state={saveState} />
        </div>
      </div>
      <Switch
        id={controlId}
        checked={checked}
        disabled={disabled}
        aria-label={label}
        aria-describedby={describedBy}
        onCheckedChange={onChange}
      />
    </div>
  );
}

export function DesktopPreferencesCard() {
  const { t } = useTranslation();
  const mac = isMac();
  const location = t(
    mac ? "preferences.desktop.menuBar" : "preferences.desktop.systemTray",
  );
  const preferences = useDesktopPreferences();
  const save = useSaveDesktopPreferences();
  const headingId = useId();
  const [saveTarget, setSaveTarget] = useState<PreferenceName | null>(null);
  const currentState: PreferenceSaveState = save.isPending
    ? "saving"
    : save.isError
      ? "error"
      : save.isSuccess
        ? "saved"
        : "idle";
  const stateFor = (name: PreferenceName): PreferenceSaveState =>
    saveTarget === name ? currentState : "idle";

  function commit(name: PreferenceName, next: DesktopPreferences): void {
    setSaveTarget(name);
    save.mutate(next);
  }

  return (
    <section aria-labelledby={headingId}>
      <Card padding="none" className="overflow-hidden rounded-xl">
        <header className="border-b border-hairline p-5">
          <h3 id={headingId} className="text-heading text-content">
            {t("preferences.desktop.title")}
          </h3>
          <p className="mt-1 text-body text-content-muted">
            {t("preferences.desktop.description")}
          </p>
        </header>

        {preferences.isPending ? (
          <DetectionStatus
            label={t("preferences.desktop.loading")}
            className="m-5"
          />
        ) : preferences.data ? (
          <div className="px-5">
            <DesktopPreferenceRow
              icon={Rocket}
              label={t("preferences.desktop.launch.label")}
              description={t(
                mac
                  ? "preferences.desktop.launch.descriptionMac"
                  : "preferences.desktop.launch.description",
              )}
              checked={preferences.data.launchOnStartup}
              disabled={save.isPending}
              saveState={stateFor("launchOnStartup")}
              onChange={(checked) =>
                commit("launchOnStartup", {
                  ...preferences.data,
                  launchOnStartup: checked,
                  silentStartup: checked
                    ? preferences.data.silentStartup
                    : false,
                })
              }
            />
            <DesktopPreferenceRow
              icon={EyeOff}
              label={t("preferences.desktop.silent.label")}
              description={
                !preferences.data.launchOnStartup
                  ? t("preferences.desktop.silent.requiresLaunch")
                  : !preferences.data.showInTray
                    ? t("preferences.desktop.requiresTray", { location })
                    : t("preferences.desktop.silent.description", { location })
              }
              checked={preferences.data.silentStartup}
              disabled={
                save.isPending ||
                !preferences.data.launchOnStartup ||
                !preferences.data.showInTray
              }
              saveState={stateFor("silentStartup")}
              onChange={(checked) =>
                commit("silentStartup", {
                  ...preferences.data,
                  silentStartup: checked,
                })
              }
            />
            <DesktopPreferenceRow
              icon={PanelTop}
              label={t("preferences.desktop.tray.label", { location })}
              description={t("preferences.desktop.tray.description", {
                location,
              })}
              checked={preferences.data.showInTray}
              disabled={save.isPending}
              saveState={stateFor("showInTray")}
              onChange={(checked) =>
                commit("showInTray", {
                  ...preferences.data,
                  showInTray: checked,
                  silentStartup: checked
                    ? preferences.data.silentStartup
                    : false,
                  minimizeToTrayOnClose: checked
                    ? preferences.data.minimizeToTrayOnClose
                    : false,
                })
              }
            />
            <DesktopPreferenceRow
              icon={Minimize2}
              label={t("preferences.desktop.close.label")}
              description={t(
                preferences.data.showInTray
                  ? "preferences.desktop.close.description"
                  : "preferences.desktop.requiresTray",
                { location },
              )}
              checked={preferences.data.minimizeToTrayOnClose}
              disabled={save.isPending || !preferences.data.showInTray}
              saveState={stateFor("minimizeToTrayOnClose")}
              onChange={(checked) =>
                commit("minimizeToTrayOnClose", {
                  ...preferences.data,
                  minimizeToTrayOnClose: checked,
                })
              }
            />
          </div>
        ) : (
          <EmptyState
            icon={PanelTop}
            title={t("preferences.desktop.error.title")}
            description={t("preferences.desktop.error.description")}
            action={
              <Button
                loading={preferences.isFetching}
                onClick={() => void preferences.refetch()}
              >
                {t("preferences.retry")}
              </Button>
            }
            className="py-8"
          />
        )}
      </Card>
    </section>
  );
}
