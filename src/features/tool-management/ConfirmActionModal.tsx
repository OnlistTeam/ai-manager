import { useRef, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ToolActionError } from "./ToolActionError";

export interface ConfirmActionModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Already-translated copy: the caller assembles the tool name; this component neither guesses nor looks it up. */
  title: string;
  description: string;
  confirmLabel: string;
  confirmTone?: "primary" | "danger";
  busy?: boolean;
  confirmDisabled?: boolean;
  error?: Error | null;
  children?: ReactNode;
  onConfirm: () => void;
}

/** A generic one-line confirmation; actions that need to show vetted technical detail progressively disclose it via children. */
export function ConfirmActionModal({
  open,
  onOpenChange,
  title,
  description,
  confirmLabel,
  confirmTone = "primary",
  busy = false,
  confirmDisabled = false,
  error = null,
  children,
  onConfirm,
}: ConfirmActionModalProps) {
  const { t } = useTranslation();
  const confirmRef = useRef<HTMLButtonElement>(null);

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      dismissible={!busy}
      initialFocusRef={confirmRef}
      title={title}
      description={description}
      size="sm"
      footer={
        <>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.cancel")}
          </Button>
          <Button
            ref={confirmRef}
            variant={confirmTone}
            disabled={confirmDisabled}
            loading={busy}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      {children}
      {error ? <ToolActionError error={error} /> : null}
    </Modal>
  );
}
