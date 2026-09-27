import { SlidersHorizontal } from "lucide-react";
import { useEffect, useId, useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ToolId } from "@/entities/tool";
import { useProductSettings } from "@/entities/settings";
import { AboutSection } from "@/features/about";
import { BackupSection } from "@/features/backup";
import { DeepLinkPasteSection } from "@/features/deep-link-import";
import { ImportSection } from "@/features/import-existing";
import { UpdateSection } from "@/features/updater";
import { Button } from "@/shared/ui/Button";
import { EmptyState } from "@/shared/ui/EmptyState";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { NetworkProxyCard } from "./NetworkProxyCard";
import { DesktopPreferencesCard } from "./DesktopPreferencesCard";
import { LanguageCard } from "./LanguageCard";
import { PrivacyCard } from "./PrivacyCard";
import {
  focusSettingsSection,
  settingsSectionProps,
  type SettingsSection,
} from "./settingsSections";
import { SettingsSkeleton } from "./SettingsSkeleton";

function SettingsGroup({
  title,
  section,
  children,
}: {
  title: string;
  /** Makes the group a target other pages can link to. */
  section?: SettingsSection;
  children: ReactNode;
}) {
  const headingId = useId();

  return (
    <section
      aria-labelledby={headingId}
      className="flex flex-col gap-2"
      {...(section ? settingsSectionProps(section) : {})}
    >
      <h2
        id={headingId}
        tabIndex={section ? -1 : undefined}
        className="text-overline text-content-muted outline-none"
      >
        {title}
      </h2>
      {children}
    </section>
  );
}

/** Spec §24's fifth page: one column, five groups — General, Network, Privacy, Backup & Restore, About. */
export interface SettingsPageProps {
  onOpenServices?: (toolId?: ToolId) => void;
  /** A section to bring into view once the page has loaded. */
  preferredSection?: SettingsSection | null;
}

export function SettingsPage({
  onOpenServices,
  preferredSection = null,
}: SettingsPageProps) {
  const { t } = useTranslation();
  const settings = useProductSettings();
  const pageRef = useRef<HTMLDivElement>(null);
  const retryButtonRef = useRef<HTMLButtonElement>(null);
  const focusAfterRetry = useRef(false);
  const settingsInitiallyLoading = settings.isPending && !settings.isFetched;
  const settingsUnavailable = settings.isFetched && settings.data === undefined;

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

  useEffect(() => {
    if (preferredSection && settings.isSuccess) {
      void focusSettingsSection(preferredSection);
    }
  }, [preferredSection, settings.isSuccess]);

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
      className="flex flex-col gap-6 outline-none"
    >
      <SectionHeader as="h1" title={t("preferences.title")} />

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
            <LanguageCard />
            <DesktopPreferencesCard />
          </SettingsGroup>

          <SettingsGroup title={t("preferences.groups.network")}>
            <NetworkProxyCard />
          </SettingsGroup>

          <SettingsGroup
            title={t("preferences.groups.privacy")}
            section="privacy"
          >
            <PrivacyCard />
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
