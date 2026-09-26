import { ChevronDown, Globe } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import {
  APP_LANGUAGES,
  LANGUAGE_ENDONYMS,
  resolveAppLanguage,
  setAppLanguage,
} from "@/i18n";
import { Card } from "@/shared/ui/Card";

/** The one general preference left: the language every page is written in. */
export function LanguageCard() {
  const { t, i18n } = useTranslation();
  const languageId = useId();
  const descriptionId = useId();
  const language = resolveAppLanguage(i18n.resolvedLanguage ?? i18n.language);

  return (
    <Card padding="none" className="overflow-hidden rounded-xl">
      <div className="grid gap-3 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-center">
        <div className="flex min-w-0 items-start gap-3">
          <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
            <Globe className="h-4 w-4" aria-hidden="true" />
          </span>
          <div className="min-w-0">
            <label
              htmlFor={languageId}
              className="text-body font-medium text-content"
            >
              {t("preferences.language.label")}
            </label>
            <p
              id={descriptionId}
              className="mt-0.5 text-caption text-content-muted"
            >
              {t("preferences.language.description")}
            </p>
          </div>
        </div>
        <div className="relative w-full sm:w-48">
          <select
            id={languageId}
            aria-describedby={descriptionId}
            value={language}
            onChange={(event) => {
              const next = APP_LANGUAGES.find(
                (value) => value === event.target.value,
              );
              if (next) void setAppLanguage(next);
            }}
            className="h-9 w-full appearance-none rounded-lg border border-hairline bg-layer-1 pl-3 pr-9 text-body text-content focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
          >
            {APP_LANGUAGES.map((value) => (
              <option key={value} value={value}>
                {LANGUAGE_ENDONYMS[value]}
              </option>
            ))}
          </select>
          <ChevronDown
            className="pointer-events-none absolute right-3 top-2.5 h-4 w-4 text-content-muted"
            aria-hidden="true"
          />
        </div>
      </div>
    </Card>
  );
}
