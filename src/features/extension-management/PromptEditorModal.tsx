import { FileClock, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import type { Extension } from "@/entities/extension";
import { usePromptDetail } from "@/entities/prompt";
import type { ToolId } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { Field } from "@/shared/ui/Field";
import { Input } from "@/shared/ui/Input";
import { Modal } from "@/shared/ui/Modal";
import { Textarea } from "@/shared/ui/Textarea";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import {
  EMPTY_PROMPT_FORM,
  promptDraftFrom,
  validatePromptForm,
  type PromptFormError,
  type PromptFormErrors,
  type PromptFormValues,
} from "./promptForm";
import { PromptMutationError } from "./PromptMutationError";
import { useSavePrompt } from "./usePromptMutations";

const FORM_ID = "prompt-editor-form";

interface PromptEditorModalProps {
  open: boolean;
  tool: ToolId;
  toolName: string;
  prompt: Extension | null;
  mutationsBlocked: boolean;
  onOpenChange: (open: boolean) => void;
}

function validationKey(
  field: keyof PromptFormErrors,
  error: PromptFormError | undefined,
): string | undefined {
  if (!error) return undefined;
  return `extensions.prompt.editor.validation.${field}.${error}`;
}

export function PromptEditorModal({
  open,
  tool,
  toolName,
  prompt,
  mutationsBlocked,
  onOpenChange,
}: PromptEditorModalProps) {
  const { t } = useTranslation();
  const [values, setValues] = useState<PromptFormValues>(EMPTY_PROMPT_FORM);
  const [errors, setErrors] = useState<PromptFormErrors>({});
  const hydratedId = useRef<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const detail = usePromptDetail(tool, prompt?.id ?? null, open);
  const savePrompt = useSavePrompt();
  const { reset: resetSavePrompt } = savePrompt;
  const editing = prompt !== null;
  const loading = editing && detail.isPending;
  const readFailed = editing && detail.isError;
  const readBlocked = loading || readFailed;
  const submitBlocked = mutationsBlocked || readBlocked;

  useEffect(() => {
    if (!open) return;
    hydratedId.current = null;
    setValues(EMPTY_PROMPT_FORM);
    setErrors({});
    resetSavePrompt();
  }, [open, prompt?.id, resetSavePrompt]);

  useEffect(() => {
    if (!open || !detail.data || hydratedId.current === detail.data.id) return;
    hydratedId.current = detail.data.id;
    setValues({
      name: detail.data.name,
      description: detail.data.description ?? "",
      content: detail.data.content,
    });
  }, [detail.data, open]);

  function update<Key extends keyof PromptFormValues>(
    key: Key,
    value: PromptFormValues[Key],
  ): void {
    setValues((current) => ({ ...current, [key]: value }));
    setErrors((current) => ({ ...current, [key]: undefined }));
  }

  function submit(event: FormEvent): void {
    event.preventDefault();
    const nextErrors = validatePromptForm(values);
    setErrors(nextErrors);
    if (Object.keys(nextErrors).length > 0 || submitBlocked) return;
    savePrompt.mutate(
      { tool, promptId: prompt?.id ?? null, draft: promptDraftFrom(values) },
      { onSuccess: () => onOpenChange(false) },
    );
  }

  const title = editing
    ? t("extensions.prompt.editor.editTitle", { name: prompt.name })
    : t("extensions.prompt.editor.createTitle");

  return (
    <Modal
      open={open}
      onOpenChange={(next) => {
        if (!next && savePrompt.isPending) return;
        onOpenChange(next);
      }}
      title={title}
      description={t("extensions.prompt.editor.description", {
        tool: toolName,
      })}
      size="lg"
      dismissible={!savePrompt.isPending}
      initialFocusRef={nameRef}
      footer={
        <>
          <Button
            variant="secondary"
            disabled={savePrompt.isPending}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            type="submit"
            form={FORM_ID}
            loading={savePrompt.isPending}
            disabled={submitBlocked}
          >
            {t("extensions.prompt.editor.save")}
          </Button>
        </>
      }
    >
      <form id={FORM_ID} className="flex flex-col gap-4" onSubmit={submit}>
        {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}

        {prompt?.enabled ? (
          <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-3 text-caption text-content-muted">
            <FileClock
              className="mt-0.5 h-4 w-4 shrink-0 text-warning"
              aria-hidden="true"
            />
            <span>{t("extensions.prompt.editor.activeBackup")}</span>
          </div>
        ) : null}

        {detail.isError ? (
          <div className="flex flex-col gap-3">
            <PromptMutationError
              error={detail.error}
              titleKey="extensions.prompt.editor.readErrorTitle"
            />
            <Button
              variant="secondary"
              loading={detail.isFetching}
              onClick={() => void detail.refetch()}
            >
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              {t("extensions.prompt.editor.retryRead")}
            </Button>
          </div>
        ) : null}

        {savePrompt.isError ? (
          <PromptMutationError
            error={savePrompt.error}
            titleKey="extensions.prompt.editor.saveErrorTitle"
          />
        ) : null}

        <Field
          id="prompt-name"
          label={t("extensions.prompt.editor.name")}
          error={
            errors.name
              ? t(validationKey("name", errors.name) ?? "")
              : undefined
          }
        >
          <Input
            ref={nameRef}
            id="prompt-name"
            value={values.name}
            disabled={readBlocked || savePrompt.isPending}
            invalid={Boolean(errors.name)}
            onChange={(event) => update("name", event.target.value)}
          />
        </Field>

        <Field
          id="prompt-description"
          label={t("extensions.prompt.editor.optionalDescription")}
          error={
            errors.description
              ? t(validationKey("description", errors.description) ?? "")
              : undefined
          }
        >
          <Input
            id="prompt-description"
            value={values.description}
            disabled={readBlocked || savePrompt.isPending}
            invalid={Boolean(errors.description)}
            onChange={(event) => update("description", event.target.value)}
          />
        </Field>

        <Field
          id="prompt-content"
          label={t("extensions.prompt.editor.content")}
          hint={t("extensions.prompt.editor.contentHint")}
          error={
            errors.content
              ? t(validationKey("content", errors.content) ?? "")
              : undefined
          }
        >
          <Textarea
            id="prompt-content"
            rows={12}
            value={values.content}
            disabled={readBlocked || savePrompt.isPending}
            invalid={Boolean(errors.content)}
            spellCheck={false}
            onChange={(event) => update("content", event.target.value)}
          />
        </Field>
      </form>
    </Modal>
  );
}
