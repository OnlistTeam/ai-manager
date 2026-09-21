import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";

interface ProviderCustomEntryBannerProps {
  busy: boolean;
  onCustomEntry: () => void;
}

/** Nudges into the direct Base URL form from the top of the preset flow. */
export function ProviderCustomEntryBanner({
  busy,
  onCustomEntry,
}: ProviderCustomEntryBannerProps) {
  const { t } = useTranslation();
  return (
    <div className="mb-3 flex items-center justify-between gap-3 rounded-xl border border-hairline bg-layer-1 p-3">
      <p className="text-caption leading-5 text-content-muted">
        {t("services.connect.customEntryHint")}
      </p>
      <Button
        type="button"
        variant="secondary"
        disabled={busy}
        onClick={onCustomEntry}
      >
        {t("services.connect.customEntry")}
      </Button>
    </div>
  );
}
