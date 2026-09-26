import { Zap } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Field } from "@/shared/ui/Field";
import { CopyableInput } from "@/shared/ui/CopyableInput";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { cn } from "@/shared/ui/cn";
import { ProviderEndpointRoutesModal } from "./ProviderEndpointRoutesModal";
import {
  normalizeProviderEndpoint,
  trailingVersionSegment,
} from "./providerEndpointRouteUtils";
import type { useProviderEndpointRoutes } from "./useProviderEndpointRoutes";

type EndpointForm = ReturnType<typeof useProviderEndpointRoutes>;

interface ProviderEndpointFieldsProps {
  disabled: boolean;
  /** `ProviderEditProfile.baseUrlTakesNoVersion`. */
  takesNoVersion: boolean;
  form: EndpointForm;
  onChange: () => void;
}

export function ProviderEndpointFields({
  disabled,
  takesNoVersion,
  form,
  onChange,
}: ProviderEndpointFieldsProps) {
  const { t } = useTranslation();
  const [routesOpen, setRoutesOpen] = useState(false);
  const versionSegment = takesNoVersion
    ? trailingVersionSegment(form.baseUrl)
    : null;
  const baseUrlWarning =
    versionSegment === null
      ? undefined
      : t("services.form.baseUrlVersionDoubled", { segment: versionSegment });
  // The same validation flag also reports a bad route typed in the routes
  // dialog, so it only speaks here when this address is the one at fault.
  const baseUrlInvalid =
    form.validation === "invalid" &&
    form.baseUrl.trim() !== "" &&
    normalizeProviderEndpoint(form.baseUrl) === null;

  return (
    <>
      <Field
        id="service-base-url"
        label={t("services.form.baseUrl")}
        hint={baseUrlWarning ? undefined : t("services.form.baseUrlHint")}
        warning={baseUrlWarning}
        error={baseUrlInvalid ? t("services.form.routeInvalid") : undefined}
        labelAction={
          <button
            type="button"
            disabled={disabled}
            className={cn(
              "flex items-center gap-1 rounded text-caption text-content-muted transition-colors hover:text-content disabled:opacity-60",
              FOCUS_RING,
            )}
            onClick={() => setRoutesOpen(true)}
          >
            <Zap className="h-3.5 w-3.5" aria-hidden="true" />
            {t("services.form.routesManage")}
          </button>
        }
      >
        <CopyableInput
          id="service-base-url"
          copyLabel={t("services.form.baseUrl")}
          type="url"
          value={form.baseUrl}
          maxLength={2_048}
          spellCheck={false}
          disabled={disabled}
          invalid={baseUrlInvalid}
          className="font-mono"
          onChange={(event) => {
            form.setBaseUrl(event.target.value);
            onChange();
          }}
        />
      </Field>

      <ProviderEndpointRoutesModal
        open={routesOpen}
        disabled={disabled}
        form={form}
        onOpenChange={setRoutesOpen}
        onChange={onChange}
      />
    </>
  );
}
