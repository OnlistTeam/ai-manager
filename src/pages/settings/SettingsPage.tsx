import { SlidersHorizontal } from "lucide-react";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ToolId } from "@/entities/tool";
import {
  useProductSettings,
  useSaveProductSettings,
} from "@/entities/settings";
import { AboutSection } from "@/features/about";
import { BackupSection } from "@/features/backup";
import { DeepLinkPasteSection } from "@/features/deep-link-import";
import { ImportSection } from "@/features/import-existing";
import { UpdateSection } from "@/features/updater";
import { Button } from "@/shared/ui/Button";
import { EmptyState } from "@/shared/ui/EmptyState";
import type { PreferenceSaveState } from "@/shared/ui/PreferenceSaveStatus";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { AppExperienceCard } from "./AppExperienceCard";
import { DownloadProxyCard } from "./DownloadProxyCard";
import { DesktopPreferencesCard } from "./DesktopPreferencesCard";
import { SettingsSkeleton } from "./SettingsSkeleton";

function SettingsGroup({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  const headingId = useId();

  return (
    <section aria-labelledby={headingId} className="flex flex-col gap-4">
      <h2 id={headingId} className="text-overline text-content-muted">
        {title}
      </h2>
      {children}
    </section>
  );
}

/** Spec §24's fifth page: one column, four groups — General, Network, Backup & Restore, About. */
export interface SettingsPageProps {
  onOpenServices?: (toolId?: ToolId) => void;
}

export function SettingsPage({ onOpenServices }: SettingsPageProps) {
  const { t } = useTranslation();
  const settings = useProductSettings();
  const save = useSaveProductSettings();
  const pageRef = useRef<HTMLDivElement>(null);
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);
  const [saveTarget, setSaveTarget] = useState<"providerFailover" | null>(null);
  const settingsInitiallyLoading = settings.isPending && !settings.isFetched;
  const settingsUnavailable = settings.isFetched && settings.data === undefined;
  const automaticProviderFailover =
    settings.data?.automaticProviderFailover ?? false;
  const currentSaveState: PreferenceSaveState = save.isPending
    ? "saving"
    : save.isError
      ? "error"
      : save.isSuccess
        ? "saved"
        : "idle";
  const automaticProviderFailoverSaveState =
    saveTarget === "providerFailover" ? currentSaveState : "idle";

  useEffect(() => {
    if (!focusAfterRetry.current || settings.isFetching) return;
    focusAfterRetry.current = false;

    const active = document.activeElement;
    if (active !== document.body && active !== retryButtonRef.current) return;
    if (settings.isSuccess) {
      pageRef.current?.focus({ preventScroll: true });
    } else if (settings.isError) {
      retryButtonRef.current?.focus({ preventScroll: true });
    }
  }, [settings.isError, settings.isFetching, settings.isSuccess]);

  function retrySettings(): void {
    focusAfterRetry.current = true;
    void settings.refetch();
  }

  return (
    <div
      ref={pageRef}
      role="region"
      aria-label={t("preferences.title")}
      tabIndex={-1}
      className="flex flex-col gap-8 outline-none"
    >
      <SectionHeader
        as="h1"
        title={t("preferences.title")}
        description={t("preferences.description")}
      />

      {settingsInitiallyLoading ? (
        <SettingsSkeleton label={t("preferences.loading")} />
      ) : null}

      {settingsUnavailable ? (
        <div
          role="alert"
          aria-label={t("preferences.error.title")}
          aria-busy={settings.isFetching || undefined}
        >
          <EmptyState
            icon={SlidersHorizontal}
            title={t("preferences.error.title")}
            description={t("preferences.error.description")}
            action={
              <Button
                ref={retryButtonRef}
                loading={settings.isFetching}
                onClick={retrySettings}
              >
                {t("preferences.retry")}
              </Button>
            }
          />
        </div>
      ) : null}

      {settings.isSuccess ? (
        <>
          <SettingsGroup title={t("preferences.groups.general")}>
            <AppExperienceCard
              automaticProviderFailover={automaticProviderFailover}
              automaticProviderFailoverSaveState={
                automaticProviderFailoverSaveState
              }
              onAutomaticProviderFailoverChange={(checked) => {
                setSaveTarget("providerFailover");
                save.mutate({ automaticProviderFailover: checked });
              }}
            />
            <DesktopPreferencesCard />
          </SettingsGroup>

          <SettingsGroup title={t("preferences.groups.network")}>
            <DownloadProxyCard />
          </SettingsGroup>

          <SettingsGroup title={t("preferences.groups.backup")}>
            <BackupSection onReviewServices={onOpenServices} />
            <ImportSection />
            <DeepLinkPasteSection />
          </SettingsGroup>

          <SettingsGroup title={t("preferences.groups.about")}>
            <UpdateSection />
            <AboutSection />
          </SettingsGroup>
        </>
      ) : null}
    </div>
  );
}
