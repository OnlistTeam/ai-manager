import { AlertCircle, EyeOff } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { Input } from "@/shared/ui/Input";
import {
  PreferenceSaveStatus,
  type PreferenceSaveState,
} from "@/shared/ui/PreferenceSaveStatus";
import { PreferenceRow } from "./PreferenceRow";
import {
  formatPrivacyWords,
  parsePrivacyWords,
  sameWords,
} from "./privacyWords";

export interface PrivacyWordsRowProps {
  words: readonly string[];
  saving: boolean;
  saveState: PreferenceSaveState;
  /** The backend's reason when the last save of the words failed. */
  errorKey: string | null;
  onSave: (words: string[]) => void;
}

/** The user's own words to hide, saved when the field loses focus or on Enter. */
export function PrivacyWordsRow({
  words,
  saving,
  saveState,
  errorKey,
  onSave,
}: PrivacyWordsRowProps) {
  const { t } = useTranslation();
  const controlId = useId();
  const descriptionId = useId();
  const statusId = useId();
  const stored = formatPrivacyWords(words);
  const [draft, setDraft] = useState(stored);

  // A save reads the normalized list back; show it the way it is stored.
  useEffect(() => setDraft(stored), [stored]);

  function commit(): void {
    if (saving) return;
    const next = parsePrivacyWords(draft);
    if (sameWords(next, words)) {
      setDraft(stored);
      return;
    }
    onSave(next);
  }

  const status = errorKey ? (
    <p
      id={statusId}
      role="alert"
      className="mt-1.5 flex items-start gap-1.5 text-caption leading-5 text-danger"
    >
      <AlertCircle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      {t(errorKey)}
    </p>
  ) : (
    <PreferenceSaveStatus id={statusId} state={saveState} />
  );
  const describedBy =
    saveState === "idle" && !errorKey
      ? descriptionId
      : `${descriptionId} ${statusId}`;

  return (
    <PreferenceRow
      icon={EyeOff}
      label={t("preferences.privacy.words.label")}
      description={t("preferences.privacy.words.description")}
      controlId={controlId}
      descriptionId={descriptionId}
      status={status}
      control={
        <Input
          id={controlId}
          value={draft}
          invalid={Boolean(errorKey)}
          spellCheck={false}
          autoCapitalize="off"
          autoComplete="off"
          aria-describedby={describedBy}
          placeholder={t("preferences.privacy.words.placeholder")}
          className="sm:w-72"
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            commit();
          }}
        />
      }
    />
  );
}
