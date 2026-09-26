import { KeyRound, ShieldCheck, UserRound } from "lucide-react";
import { useId } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import { PreferenceSwitchRow } from "./PreferenceRow";
import { PrivacyWordsRow } from "./PrivacyWordsRow";
import { usePrivacySettings } from "./usePrivacySettings";

/**
 * What privacy protection hides from the service (ADR-0049): keys and
 * passwords, personal information, and the user's own words. It only covers
 * requests that go through AI Manager.
 */
export function PrivacyCard() {
  const { t } = useTranslation();
  const headingId = useId();
  const privacy = usePrivacySettings();
  const data = privacy.data;

  return (
    <section aria-labelledby={headingId}>
      <Card padding="none" className="overflow-hidden rounded-xl">
        <header className="border-b border-hairline px-4 py-3">
          <h3 id={headingId} className="text-body font-medium text-content">
            {t("preferences.privacy.title")}
          </h3>
          <p className="mt-0.5 text-caption text-content-muted">
            {t("preferences.privacy.description")}
          </p>
        </header>

        {data ? (
          <div className="px-4">
            <PreferenceSwitchRow
              icon={KeyRound}
              label={t("preferences.privacy.secrets.label")}
              description={t("preferences.privacy.secrets.description")}
              checked={data.maskSecrets}
              disabled={privacy.saving}
              saveState={privacy.stateFor("maskSecrets")}
              onChange={(checked) =>
                privacy.save("maskSecrets", { maskSecrets: checked })
              }
            />
            <PreferenceSwitchRow
              icon={UserRound}
              label={t("preferences.privacy.personal.label")}
              description={t("preferences.privacy.personal.description")}
              checked={data.maskPersonal}
              disabled={privacy.saving}
              saveState={privacy.stateFor("maskPersonal")}
              onChange={(checked) =>
                privacy.save("maskPersonal", { maskPersonal: checked })
              }
            />
            <PrivacyWordsRow
              words={data.words}
              saving={privacy.saving}
              saveState={privacy.stateFor("words")}
              errorKey={privacy.errorKeyFor("words")}
              onSave={(words) => privacy.save("words", { words })}
            />
          </div>
        ) : privacy.loading ? (
          <DetectionStatus
            label={t("preferences.privacy.loading")}
            className="m-4"
          />
        ) : (
          <EmptyState
            icon={ShieldCheck}
            title={t("preferences.privacy.error.title")}
            description={t("preferences.privacy.error.description")}
            action={
              <Button loading={privacy.retrying} onClick={privacy.retry}>
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
