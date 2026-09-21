import {
  CheckCircle2,
  ChevronDown,
  Globe,
  Shuffle,
  Sparkles,
} from "lucide-react";
import { useId, type ComponentType, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  APP_LANGUAGES,
  LANGUAGE_ENDONYMS,
  resolveAppLanguage,
  setAppLanguage,
  type AppLanguage,
} from "@/i18n";
import { Card } from "@/shared/ui/Card";
import { Switch } from "@/shared/ui/Switch";
import {
  PreferenceSaveStatus,
  type PreferenceSaveState,
} from "@/shared/ui/PreferenceSaveStatus";

interface PreferenceRowProps {
  icon: ComponentType<{ className?: string }>;
  label: string;
  description: string;
  descriptionId?: string;
  controlId?: string;
  status?: ReactNode;
  children: ReactNode;
}

function PreferenceRow({
  icon: Icon,
  label,
  description,
  descriptionId,
  controlId,
  status,
  children,
}: PreferenceRowProps) {
  return (
    <div className="grid gap-3 border-b border-hairline py-4 last:border-b-0 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
      <div className="flex min-w-0 items-start gap-3">
        <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
          <Icon className="h-4 w-4" aria-hidden="true" />
        </span>
        <div className="min-w-0">
          {controlId ? (
            <label
              htmlFor={controlId}
              className="text-body font-medium text-content"
            >
              {label}
            </label>
          ) : (
            <p className="text-body font-medium text-content">{label}</p>
          )}
          <p
            id={descriptionId}
            className="mt-0.5 text-caption text-content-muted"
          >
            {description}
          </p>
          {status}
        </div>
      </div>
      <div className="min-w-0 sm:justify-self-end">{children}</div>
    </div>
  );
}

export interface AppExperienceCardProps {
  automaticProviderFailover: boolean;
  automaticProviderFailoverSaveState: PreferenceSaveState;
  onAutomaticProviderFailoverChange: (checked: boolean) => void;
}

export function AppExperienceCard({
  automaticProviderFailover,
  automaticProviderFailoverSaveState,
  onAutomaticProviderFailoverChange,
}: AppExperienceCardProps) {
  const { t, i18n } = useTranslation();
  const headingId = useId();
  const languageId = useId();
  const failoverId = useId();
  const languageDescriptionId = useId();
  const failoverDescriptionId = useId();
  const failoverStatusId = useId();
  const language = resolveAppLanguage(i18n.resolvedLanguage ?? i18n.language);
  const failoverDescribedBy =
    automaticProviderFailoverSaveState === "idle"
      ? failoverDescriptionId
      : `${failoverDescriptionId} ${failoverStatusId}`;
  const preferenceSaving = automaticProviderFailoverSaveState === "saving";

  const languages: readonly {
    value: AppLanguage;
    label: string;
  }[] = APP_LANGUAGES.map((value) => ({
    value,
    label: LANGUAGE_ENDONYMS[value],
  }));
  return (
    <section aria-labelledby={headingId}>
      <Card
        padding="none"
        className="overflow-hidden rounded-xl border-brand/10 shadow-md"
      >
        <header className="relative overflow-hidden border-b border-hairline bg-layer-1 p-5">
          <span
            className="pointer-events-none absolute -right-16 -top-20 h-56 w-56 rounded-full border border-brand/10 bg-brand/5"
            aria-hidden="true"
          />
          <div className="relative flex items-start justify-between gap-4">
            <div className="max-w-2xl">
              <p className="flex items-center gap-2 text-caption font-medium text-brand">
                <Sparkles className="h-4 w-4" aria-hidden="true" />
                {t("preferences.experience.eyebrow")}
              </p>
              <h3 id={headingId} className="mt-1 text-heading text-content">
                {t("preferences.experience.title")}
              </h3>
              <p className="mt-1 text-body text-content-muted">
                {t("preferences.experience.description")}
              </p>
            </div>
            <span className="hidden shrink-0 items-center gap-1.5 rounded-full border border-brand/10 bg-brand/10 px-2.5 py-1 text-caption font-medium text-content sm:inline-flex">
              <CheckCircle2
                className="h-3.5 w-3.5 text-success"
                aria-hidden="true"
              />
              {t("preferences.experience.instant")}
            </span>
          </div>
        </header>

        <div className="px-5">
          <PreferenceRow
            icon={Globe}
            label={t("preferences.experience.language.label")}
            description={t("preferences.experience.language.description")}
            descriptionId={languageDescriptionId}
            controlId={languageId}
          >
            <div className="relative w-full sm:w-48">
              <select
                id={languageId}
                aria-describedby={languageDescriptionId}
                value={language}
                onChange={(event) => {
                  const next = languages.find(
                    ({ value }) => value === event.target.value,
                  );
                  if (next) void setAppLanguage(next.value);
                }}
                className="h-10 w-full appearance-none rounded-lg border border-hairline bg-layer-1 pl-3 pr-9 text-body text-content focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
              >
                {languages.map(({ value, label }) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
              <ChevronDown
                className="pointer-events-none absolute right-3 top-3 h-4 w-4 text-content-muted"
                aria-hidden="true"
              />
            </div>
          </PreferenceRow>

          <PreferenceRow
            icon={Shuffle}
            label={t("preferences.providerFailover.label")}
            description={t("preferences.providerFailover.description")}
            descriptionId={failoverDescriptionId}
            controlId={failoverId}
            status={
              <PreferenceSaveStatus
                id={failoverStatusId}
                state={automaticProviderFailoverSaveState}
              />
            }
          >
            <Switch
              id={failoverId}
              checked={automaticProviderFailover}
              disabled={preferenceSaving}
              aria-label={t("preferences.providerFailover.label")}
              aria-describedby={failoverDescribedBy}
              onCheckedChange={onAutomaticProviderFailoverChange}
            />
          </PreferenceRow>
        </div>
      </Card>
    </section>
  );
}
