import { useId, useState } from "react";
import { ClipboardPaste } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useSubmitPastedDeepLink } from "@/entities/deeplink";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { DeepLinkErrorNotice } from "./DeepLinkErrorNotice";

/**
 * The paste path (ADR-0029 decision 4). A link the user brought from their own
 * clipboard may carry a key, because it never travelled through argv, a process
 * list or a system log. Accepting it queues it like any other link, so the same
 * confirmation dialog decides what happens next.
 */
export function DeepLinkPasteSection() {
  const { t } = useTranslation();
  const fieldId = useId();
  const [link, setLink] = useState("");
  const submit = useSubmitPastedDeepLink();
  const trimmed = link.trim();
  const error = submit.isError ? toErrorCopy(submit.error) : null;

  function accept() {
    if (!trimmed || submit.isPending) return;
    submit.mutate(trimmed, { onSuccess: () => setLink("") });
  }

  return (
    <section
      aria-label={t("deeplink.paste.title")}
      className="flex flex-col gap-3"
    >
      <Card padding="none" className="rounded-xl">
        <div className="flex flex-col gap-3 p-4">
          <div className="flex min-w-0 items-start gap-3">
            <span className="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-layer-1 text-brand">
              <ClipboardPaste className="h-4 w-4" aria-hidden="true" />
            </span>
            <div className="min-w-0">
              <h3 className="text-body font-medium text-content">
                {t("deeplink.paste.title")}
              </h3>
              <p className="mt-0.5 text-caption leading-5 text-content-muted">
                {t("deeplink.paste.description")}
              </p>
            </div>
          </div>

          <div className="flex flex-wrap items-end gap-3">
            <Field
              id={fieldId}
              label={t("deeplink.paste.label")}
              hint={t("deeplink.paste.hint")}
              className="min-w-[16rem] flex-1"
            >
              <Input
                id={fieldId}
                value={link}
                spellCheck={false}
                autoComplete="off"
                invalid={error !== null}
                placeholder={t("deeplink.paste.placeholder")}
                onChange={(event) => {
                  setLink(event.target.value);
                  if (submit.isError) submit.reset();
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    accept();
                  }
                }}
              />
            </Field>
            <Button
              variant="secondary"
              loading={submit.isPending}
              disabled={trimmed.length === 0}
              onClick={accept}
            >
              {t("deeplink.paste.action")}
            </Button>
          </div>
        </div>
      </Card>

      {error ? <DeepLinkErrorNotice error={error} /> : null}
    </section>
  );
}
