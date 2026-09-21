import { ShieldCheck } from "lucide-react";
import type { RefObject } from "react";
import { useTranslation } from "react-i18next";
import { Field } from "@/shared/ui/Field";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { Input } from "@/shared/ui/Input";
import { cn } from "@/shared/ui/cn";
import type {
  McpInstallErrors,
  McpInstallValues,
  McpTransport,
} from "./mcpInstallForm";
import { McpTransportPicker } from "./McpTransportPicker";

const NAME_ID = "mcp-install-name";
const DESCRIPTION_ID = "mcp-install-description";
const COMMAND_ID = "mcp-install-command";
const ARGUMENTS_ID = "mcp-install-arguments";
const URL_ID = "mcp-install-url";

interface McpInstallFieldsProps {
  values: McpInstallValues;
  errors: McpInstallErrors;
  disabled: boolean;
  nameRef: RefObject<HTMLInputElement>;
  commandRef: RefObject<HTMLInputElement>;
  urlRef: RefObject<HTMLInputElement>;
  onFieldChange: (field: keyof McpInstallValues, value: string) => void;
  onTransportChange: (transport: McpTransport) => void;
}

export function McpInstallFields({
  values,
  errors,
  disabled,
  nameRef,
  commandRef,
  urlRef,
  onFieldChange,
  onTransportChange,
}: McpInstallFieldsProps) {
  const { t } = useTranslation();

  return (
    <>
      <Field
        id={NAME_ID}
        label={t("extensions.mcp.install.name")}
        hint={t("extensions.mcp.install.nameHint")}
        error={errors.name ? t(errors.name) : undefined}
      >
        <Input
          ref={nameRef}
          id={NAME_ID}
          value={values.name}
          maxLength={80}
          invalid={Boolean(errors.name)}
          disabled={disabled}
          autoComplete="off"
          onChange={(event) => onFieldChange("name", event.target.value)}
        />
      </Field>

      <McpTransportPicker
        value={values.transport}
        disabled={disabled}
        onChange={onTransportChange}
      />

      {values.transport === "stdio" ? (
        <div className="grid gap-4 sm:grid-cols-2">
          <Field
            id={COMMAND_ID}
            label={t("extensions.mcp.install.command")}
            hint={t("extensions.mcp.install.commandHint")}
            error={errors.command ? t(errors.command) : undefined}
          >
            <Input
              ref={commandRef}
              id={COMMAND_ID}
              value={values.command}
              maxLength={512}
              spellCheck={false}
              invalid={Boolean(errors.command)}
              disabled={disabled}
              placeholder={t("extensions.mcp.install.commandPlaceholder")}
              className="font-mono"
              onChange={(event) => onFieldChange("command", event.target.value)}
            />
          </Field>
          <Field
            id={ARGUMENTS_ID}
            label={t("extensions.mcp.install.arguments")}
            hint={t("extensions.mcp.install.argumentsHint")}
            error={errors.argumentsText ? t(errors.argumentsText) : undefined}
          >
            <textarea
              id={ARGUMENTS_ID}
              rows={3}
              value={values.argumentsText}
              disabled={disabled}
              aria-invalid={Boolean(errors.argumentsText) || undefined}
              placeholder={t("extensions.mcp.install.argumentsPlaceholder")}
              className={cn(
                "min-h-24 w-full resize-y rounded-md border bg-layer-1 px-3 py-2 font-mono text-body text-content",
                "placeholder:text-content-muted disabled:cursor-not-allowed disabled:opacity-60",
                errors.argumentsText ? "border-danger" : "border-hairline",
                FOCUS_RING,
              )}
              onChange={(event) =>
                onFieldChange("argumentsText", event.target.value)
              }
            />
          </Field>
        </div>
      ) : (
        <Field
          id={URL_ID}
          label={t("extensions.mcp.install.url")}
          hint={t("extensions.mcp.install.urlHint")}
          error={errors.url ? t(errors.url) : undefined}
        >
          <Input
            ref={urlRef}
            id={URL_ID}
            type="url"
            value={values.url}
            maxLength={2_048}
            spellCheck={false}
            invalid={Boolean(errors.url)}
            disabled={disabled}
            placeholder={t("extensions.mcp.install.urlPlaceholder")}
            onChange={(event) => onFieldChange("url", event.target.value)}
          />
        </Field>
      )}

      <Field
        id={DESCRIPTION_ID}
        label={t("extensions.mcp.install.optionalDescription")}
        error={errors.description ? t(errors.description) : undefined}
      >
        <textarea
          id={DESCRIPTION_ID}
          rows={2}
          value={values.description}
          maxLength={240}
          disabled={disabled}
          aria-invalid={Boolean(errors.description) || undefined}
          className={cn(
            "w-full resize-none rounded-md border bg-layer-1 px-3 py-2 text-body text-content",
            "disabled:cursor-not-allowed disabled:opacity-60",
            errors.description ? "border-danger" : "border-hairline",
            FOCUS_RING,
          )}
          onChange={(event) => onFieldChange("description", event.target.value)}
        />
      </Field>

      <aside className="flex items-start gap-2 rounded-lg bg-layer-1 px-3 py-2.5 text-caption text-content-muted">
        <ShieldCheck
          className="mt-0.5 h-4 w-4 shrink-0 text-brand"
          aria-hidden="true"
        />
        <p>
          {t(
            values.transport === "stdio"
              ? "extensions.mcp.install.trustLocal"
              : "extensions.mcp.install.trustRemote",
          )}{" "}
          {t("extensions.mcp.install.noSecrets")}
        </p>
      </aside>
    </>
  );
}
