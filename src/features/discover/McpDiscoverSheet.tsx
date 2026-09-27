import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { DiscoverMcpServer } from "@/entities/discover";
import type { ExtensionScopeOption } from "@/features/extension-management";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { Modal } from "@/shared/ui/Modal";
import { DiscoverAppChoice } from "./DiscoverAppChoice";
import { DiscoverSheetHead } from "./DiscoverSheetHead";
import { inputLabel, runsLabel, serverDescription } from "./discoverLabels";
import type { DiscoverAddState } from "./useDiscoverAdds";
import { useOpenDiscoverLink } from "./useDiscoverMutations";

export interface McpDiscoverSheetProps {
  server: DiscoverMcpServer;
  reachable: readonly ExtensionScopeOption[];
  unreachable: readonly string[];
  state: DiscoverAddState;
  blocked: boolean;
  onClose: () => void;
  /** Resolves to `null` when the task started, else to the refusal. */
  onAdd: (
    values: Record<string, string>,
    scopes: readonly ExtensionScopeOption[],
  ) => Promise<unknown>;
}

/**
 * One server up close: what it is, what it needs, and which apps get it.
 * Values are held only in this dialog and sent once, with the add.
 */
export function McpDiscoverSheet({
  server,
  reachable,
  unreachable,
  state,
  blocked,
  onClose,
  onAdd,
}: McpDiscoverSheetProps) {
  const { t } = useTranslation();
  const formId = useId();
  const openLink = useOpenDiscoverLink();
  const [values, setValues] = useState<Record<string, string>>({});
  const [selected, setSelected] = useState<ReadonlySet<string>>(
    () => new Set(reachable.map((target) => target.key)),
  );
  const [missing, setMissing] = useState<string | null>(null);
  const [refusal, setRefusal] = useState<unknown>(null);
  const [submitting, setSubmitting] = useState(false);
  // While this dialog's own request is out, the form stays; once a task
  // runs, or the item is already here, there is nothing left to fill in.
  const done = state === "added" || (state === "adding" && !submitting);
  const description = serverDescription(t, server);

  async function submit() {
    const empty = server.inputs.find(
      (input) => input.required && !values[input.key]?.trim(),
    );
    if (empty) {
      setMissing(empty.key);
      document.getElementById(`${formId}-${empty.key}`)?.focus();
      return;
    }
    setSubmitting(true);
    setRefusal(null);
    const refused = await onAdd(
      values,
      reachable.filter((target) => selected.has(target.key)),
    );
    setSubmitting(false);
    if (refused === null) onClose();
    else setRefusal(refused);
  }

  const refusalCopy = refusal ? toErrorCopy(refusal) : null;

  return (
    <Modal
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
      title={server.title}
      dismissible={!submitting}
      footer={
        <>
          <Button variant="secondary" onClick={onClose} disabled={submitting}>
            {done ? t("discover.sheet.close") : t("discover.sheet.cancel")}
          </Button>
          {done ? null : (
            <Button
              loading={submitting}
              disabled={
                blocked || selected.size === 0 || reachable.length === 0
              }
              onClick={() => void submit()}
            >
              {t("discover.sheet.confirm")}
            </Button>
          )}
        </>
      }
    >
      <div className="flex flex-col gap-3">
        <DiscoverSheetHead
          icon={server.icon}
          name={server.title}
          meta={
            <>
              {server.publisher ? <span>{server.publisher}</span> : null}
              <Badge>{runsLabel(t, server)}</Badge>
            </>
          }
          links={
            server.homepage
              ? [
                  {
                    label: t("discover.homepage"),
                    onOpen: () =>
                      openLink.mutate({
                        kind: "mcp",
                        id: server.id,
                        link: "homepage",
                      }),
                  },
                ]
              : []
          }
        />
        <p className="text-body text-content">
          {description ?? t("discover.noDescription")}
        </p>
        {server.signIn ? (
          <p className="text-caption text-content-muted">
            {t("discover.signIn")}
          </p>
        ) : null}
        {state === "added" && server.added ? (
          <p className="text-caption text-content-muted">
            {t("discover.addedAs", { name: server.added })}
          </p>
        ) : null}
        {done
          ? null
          : server.inputs.map((input) => {
              const id = `${formId}-${input.key}`;
              const label = inputLabel(t, input);
              const hint = [
                input.site
                  ? t("discover.input.site", { site: input.site })
                  : input.kind === "folder"
                    ? t("discover.input.folderHint")
                    : input.description,
                t(`discover.input.${input.target}`, { key: input.key }),
              ]
                .filter(Boolean)
                .join(" · ");
              return (
                <Field
                  key={input.key}
                  id={id}
                  label={
                    input.required
                      ? label
                      : t("discover.input.optional", { label })
                  }
                  hint={hint}
                  error={
                    missing === input.key
                      ? t("discover.input.missing", { label })
                      : undefined
                  }
                >
                  <Input
                    id={id}
                    type={input.secret ? "password" : "text"}
                    autoComplete="off"
                    spellCheck={false}
                    className="font-mono"
                    placeholder={input.placeholder ?? undefined}
                    value={values[input.key] ?? ""}
                    invalid={missing === input.key}
                    onChange={(event) => {
                      setMissing(null);
                      setValues((current) => ({
                        ...current,
                        [input.key]: event.target.value,
                      }));
                    }}
                  />
                </Field>
              );
            })}
        {done ? null : (
          <DiscoverAppChoice
            itemName={server.title}
            targets={reachable}
            unreachable={unreachable}
            selected={selected}
            disabled={submitting}
            onChange={setSelected}
          />
        )}
        {refusalCopy ? (
          <p role="alert" className="text-caption text-danger">
            {t(refusalCopy.messageKey)}
          </p>
        ) : null}
      </div>
    </Modal>
  );
}
