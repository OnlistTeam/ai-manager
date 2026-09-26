import { AlertCircle, Plus, X } from "lucide-react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { Input } from "@/shared/ui/Input";
import {
  mcpVariableRow,
  type McpVariableKind,
  type McpVariableRow,
} from "./mcpInstallForm";

interface McpVariableRowsProps {
  kind: McpVariableKind;
  rows: McpVariableRow[];
  error?: string;
  disabled: boolean;
  onChange: (rows: McpVariableRow[]) => void;
}

/**
 * Environment variables or request headers as name/value rows. It starts
 * with no rows, so the section costs one line until someone needs it.
 * Values are shown in plain text, like saved API keys elsewhere in the app.
 */
export function McpVariableRows({
  kind,
  rows,
  error,
  disabled,
  onChange,
}: McpVariableRowsProps) {
  const { t } = useTranslation();
  const focusRowId = useRef<string | null>(null);
  const addRef = useRef<HTMLButtonElement>(null);
  const hintId = `mcp-install-${kind}-hint`;
  const errorId = `mcp-install-${kind}-error`;

  const update = (id: string, patch: Partial<McpVariableRow>) =>
    onChange(rows.map((row) => (row.id === id ? { ...row, ...patch } : row)));
  const add = () => {
    const row = mcpVariableRow();
    focusRowId.current = row.id;
    onChange([...rows, row]);
  };
  const remove = (id: string) => {
    onChange(rows.filter((row) => row.id !== id));
    addRef.current?.focus();
  };

  return (
    <fieldset
      className="flex flex-col gap-1.5"
      aria-describedby={error ? `${hintId} ${errorId}` : hintId}
    >
      <legend className="text-caption text-content">
        {t(`extensions.mcp.install.${kind}.label`)}
      </legend>
      <p id={hintId} className="text-caption text-content-muted">
        {t(`extensions.mcp.install.${kind}.hint`)}
      </p>
      {rows.map((row) => (
        <div
          key={row.id}
          className="grid grid-cols-[minmax(0,2fr)_minmax(0,3fr)_auto] gap-2"
        >
          <Input
            ref={(element) => {
              if (element && focusRowId.current === row.id) {
                focusRowId.current = null;
                element.focus();
              }
            }}
            aria-label={t("extensions.mcp.install.variable.name")}
            value={row.name}
            maxLength={128}
            spellCheck={false}
            autoComplete="off"
            invalid={Boolean(error)}
            disabled={disabled}
            placeholder={t(`extensions.mcp.install.${kind}.namePlaceholder`)}
            className="font-mono"
            onChange={(event) => update(row.id, { name: event.target.value })}
          />
          <Input
            aria-label={t("extensions.mcp.install.variable.value")}
            value={row.value}
            maxLength={8_192}
            spellCheck={false}
            autoComplete="off"
            invalid={Boolean(error)}
            disabled={disabled}
            className="font-mono"
            onChange={(event) => update(row.id, { value: event.target.value })}
          />
          <Button
            variant="ghost"
            size="sm"
            className="h-9 px-2.5"
            disabled={disabled}
            aria-label={t("extensions.mcp.install.variable.remove")}
            onClick={() => remove(row.id)}
          >
            <X className="h-4 w-4" aria-hidden="true" />
          </Button>
        </div>
      ))}
      {error ? (
        <p
          id={errorId}
          role="alert"
          className="flex items-center gap-1.5 text-caption text-danger"
        >
          <AlertCircle className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          {t(error)}
        </p>
      ) : null}
      <Button
        ref={addRef}
        variant="ghost"
        size="xs"
        className="self-start"
        disabled={disabled || rows.length >= 64}
        onClick={add}
      >
        <Plus className="h-3.5 w-3.5" aria-hidden="true" />
        {t(`extensions.mcp.install.${kind}.add`)}
      </Button>
    </fieldset>
  );
}
