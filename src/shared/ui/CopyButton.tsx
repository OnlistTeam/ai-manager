import * as React from "react";
import { Check, Copy } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "./cn";

/** How long the copy-success feedback stays visible. Long enough to notice, short enough not to collide with the next click. */
const COPIED_FEEDBACK_MS = 1600;

export interface CopyButtonProps {
  /** The raw text to write to the clipboard. */
  value: string;
  /** A readable name for what's being copied, e.g. "API Key" — feeds the button's accessible name. */
  label: string;
  className?: string;
}

/**
 * An icon-only copy button. After success the icon swaps to a checkmark and the
 * accessible name switches to "Copied" — a screen reader announces the change
 * while focus stays on the button, so there's no need for a separate live region.
 */
export function CopyButton({ value, label, className }: CopyButtonProps) {
  const { t } = useTranslation();
  const [copied, setCopied] = React.useState(false);
  const timerRef = React.useRef<ReturnType<typeof setTimeout>>(undefined);

  React.useEffect(() => () => clearTimeout(timerRef.current), []);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
    } catch {
      // Stay silent when the clipboard write is denied: the value is already fully
      // visible right next to it, so the user can select and copy it themselves —
      // an error toast here would be more annoying than the failure itself.
      return;
    }
    setCopied(true);
    clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setCopied(false), COPIED_FEEDBACK_MS);
  };

  const name = copied
    ? t("ds.action.copied")
    : t("ds.action.copyNamed", { name: label });

  return (
    <button
      type="button"
      aria-label={name}
      title={name}
      className={cn(
        "inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-content-muted",
        "transition-colors duration-fast ease-standard hover:bg-layer-2 hover:text-content",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/50",
        copied && "text-success hover:text-success",
        className,
      )}
      onClick={copy}
    >
      {copied ? (
        <Check className="h-3.5 w-3.5" aria-hidden="true" />
      ) : (
        <Copy className="h-3.5 w-3.5" aria-hidden="true" />
      )}
    </button>
  );
}
