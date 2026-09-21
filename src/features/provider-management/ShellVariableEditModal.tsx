import { useEffect, useRef, useState } from "react";
import { FileCog } from "lucide-react";
import { useTranslation } from "react-i18next";

import type { ShellVariableLocation, ToolId } from "@/native";
import { Button } from "@/shared/ui/Button";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { Modal } from "@/shared/ui/Modal";
import { ProviderActionError } from "./ProviderActionError";
import { useWriteShellVariable } from "./useShellVariables";

export interface ShellVariableEditModalProps {
  /** `null` closes the dialog. One prop carries both openness and subject. */
  location: ShellVariableLocation | null;
  tool: ToolId | null;
  onOpenChange: (open: boolean) => void;
}

/**
 * Changes one value on one line of a shell start-up file (ADR-0042).
 *
 * The file and the line are shown rather than implied, because this is the
 * user's own dotfile and they are entitled to know which line is about to
 * change before it changes. The old value is prefilled: asking someone to
 * retype an address they cannot see is a guess, not an edit.
 *
 * The one thing the dialog has to say and cannot show is that a terminal
 * already running keeps the old value — the variable was read when that shell
 * started, and nothing this application does reaches back into it.
 */
export function ShellVariableEditModal({
  location,
  tool,
  onOpenChange,
}: ShellVariableEditModalProps) {
  const { t } = useTranslation();
  const write = useWriteShellVariable();
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);
  const identity =
    location === null ? null : `${location.path}:${location.line}`;

  useEffect(() => {
    setValue(location?.value ?? "");
    write.reset();
    // `write` is a stable mutation handle from React Query.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [identity]);

  const unchanged = location !== null && value === location.value;
  const busy = write.isPending;

  return (
    <Modal
      open={location !== null && tool !== null}
      onOpenChange={onOpenChange}
      dismissible={!busy}
      title={t("services.shellVariable.title", {
        variable: location?.variable ?? "",
      })}
      initialFocusRef={inputRef}
    >
      {location === null || tool === null ? null : (
        <div className="flex flex-col gap-4">
          <p className="flex min-w-0 items-start gap-2 text-caption text-content-muted">
            <FileCog
              className="mt-0.5 h-4 w-4 shrink-0 text-brand"
              aria-hidden="true"
            />
            <span className="min-w-0 break-all">
              {t("services.shellVariable.at", {
                path: location.path,
                line: location.line,
              })}
            </span>
          </p>

          <Field
            id="shell-variable-value"
            label={t("services.shellVariable.value")}
          >
            <Input
              id="shell-variable-value"
              ref={inputRef}
              value={value}
              disabled={busy}
              spellCheck={false}
              className="font-mono text-mono-sm"
              onChange={(event) => setValue(event.target.value)}
            />
          </Field>

          <p className="text-caption text-content-muted">
            {t("services.shellVariable.openTerminalsKeepTheOldValue")}
          </p>

          {write.error ? (
            <ProviderActionError
              title={t("services.shellVariable.title", {
                variable: location.variable,
              })}
              error={write.error}
              retryHint={t("services.shellVariable.nothingChanged")}
            />
          ) : null}

          {write.isSuccess ? (
            <p role="status" className="text-caption text-success">
              {t("services.shellVariable.saved")}
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-end gap-2">
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => onOpenChange(false)}
            >
              {t("ds.action.cancel")}
            </Button>
            <Button
              disabled={unchanged || busy}
              loading={busy}
              onClick={() =>
                write.mutate(
                  {
                    tool,
                    update: {
                      variable: location.variable,
                      line: location.line,
                      expectedValue: location.value,
                      newValue: value,
                    },
                  },
                  { onSuccess: () => onOpenChange(false) },
                )
              }
            >
              {t("services.shellVariable.save")}
            </Button>
          </div>
        </div>
      )}
    </Modal>
  );
}
