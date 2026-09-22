import * as React from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "./cn";
import { FOCUS_RING } from "./focusRing";

const SIZE: Record<NonNullable<ModalProps["size"]>, string> = {
  sm: "max-w-sm",
  md: "max-w-lg",
  lg: "max-w-2xl",
};

export interface ModalProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  size?: "sm" | "md" | "lg";
  /**
   * `false` closes the three user-driven exits at once: Escape, an overlay
   * click and the header X. Callers set it while a write is in flight so a
   * stray keypress cannot abandon a half-finished operation; the operation
   * itself still closes the dialog through `open`.
   */
  dismissible?: boolean;
  /** Optional first task target for form dialogs; the opener is still restored. */
  initialFocusRef?: React.RefObject<HTMLElement>;
  /** Safe return target when the original opener becomes disabled while open. */
  returnFocusFallbackRef?: React.RefObject<HTMLElement>;
  footer?: React.ReactNode;
  children?: React.ReactNode;
}

/**
 * Radix owns the focus trap, the Escape handling and the aria wiring; this
 * wrapper only supplies the product look: an opaque surface, one divider above
 * the actions, and padding tight enough that a three-field form does not read
 * as a page.
 */
export function Modal({
  open,
  onOpenChange,
  title,
  description,
  size = "md",
  dismissible = true,
  initialFocusRef,
  returnFocusFallbackRef,
  footer,
  children,
}: ModalProps) {
  const { t } = useTranslation();
  const returnFocusRef = React.useRef<HTMLElement | null>(null);

  /*
   * A caller that writes two conditional slots in its body —
   * `{children}{error ? <Error/> : null}` — hands this component an *array*,
   * and an array of nothing but `undefined` and `null` is still truthy. A plain
   * `children ?` check therefore let an empty body through, and because the
   * header carries a bottom rule and the footer a top one, the result was a
   * confirmation dialog with two horizontal lines and a gap between them.
   * `Children.toArray` drops exactly the values JSX uses for "render nothing",
   * so the question being asked is the one that was always meant.
   */
  const hasBody = React.Children.toArray(children).length > 0;

  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="app-modal-overlay fixed inset-0 z-50 bg-black/40 backdrop-blur-sm animate-ds-overlay-in" />
        <DialogPrimitive.Content
          // Radix warns in dev when a dialog has no description. Opting out
          // explicitly keeps the console clean without inventing copy; when a
          // description exists the prop is left alone so Radix can wire it up.
          {...(description ? {} : { "aria-describedby": undefined })}
          onOpenAutoFocus={(event) => {
            const active = document.activeElement;
            returnFocusRef.current =
              active instanceof HTMLElement && active !== document.body
                ? active
                : null;
            const initial = initialFocusRef?.current;
            if (initial?.isConnected) {
              event.preventDefault();
              initial.focus();
            }
          }}
          onCloseAutoFocus={(event) => {
            const opener = returnFocusRef.current;
            returnFocusRef.current = null;
            const target =
              opener?.isConnected && !opener.matches(":disabled")
                ? opener
                : returnFocusFallbackRef?.current;
            if (!target?.isConnected) return;

            // Controlled dialogs do not have a Radix Dialog.Trigger, so the
            // library has no trigger node to restore on its own. Returning to
            // the real opener keeps keyboard users anchored after Escape/X.
            event.preventDefault();
            target.focus();
          }}
          onEscapeKeyDown={(event) => {
            if (!dismissible) event.preventDefault();
          }}
          onInteractOutside={(event) => {
            if (!dismissible) event.preventDefault();
          }}
          className={cn(
            "app-floating-surface",
            "fixed left-1/2 top-1/2 z-50 w-[calc(100%-2rem)] -translate-x-1/2 -translate-y-1/2",
            // Border colour, background and shadow come from
            // `.app-floating-surface`; the padding here sets the corner that
            // matches it.
            "flex max-h-[calc(100vh-2rem)] flex-col overflow-hidden rounded-2xl border p-5",
            "animate-ds-dialog-in",
            SIZE[size],
          )}
        >
          <div
            className={cn(
              "app-modal-header flex shrink-0 items-start justify-between gap-4",
              // With no body between them, the header's rule and the footer's
              // rule are the same divider drawn twice. The footer keeps its
              // one, because the divider belongs above the actions.
              !hasBody && "app-modal-header--flush",
            )}
          >
            <div className="min-w-0">
              <DialogPrimitive.Title className="break-words text-title text-content">
                {title}
              </DialogPrimitive.Title>
              {description ? (
                <DialogPrimitive.Description className="mt-1 text-body text-content-muted">
                  {description}
                </DialogPrimitive.Description>
              ) : null}
            </div>
            {dismissible ? (
              <DialogPrimitive.Close
                aria-label={t("ds.action.close")}
                className={cn(
                  "app-modal-close",
                  "flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-content-muted",
                  "transition-colors duration-fast ease-standard hover:bg-layer-2 hover:text-content",
                  FOCUS_RING,
                )}
              >
                <X className="h-4 w-4" aria-hidden="true" />
              </DialogPrimitive.Close>
            ) : null}
          </div>
          {hasBody ? (
            <div className="app-modal-body scrollbar-subtle -mx-1 mt-3 min-h-0 overflow-y-auto px-1 text-body">
              {children}
            </div>
          ) : null}
          {footer ? (
            <div className="app-modal-footer mt-4 flex shrink-0 items-center justify-end gap-2">
              {footer}
            </div>
          ) : null}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
