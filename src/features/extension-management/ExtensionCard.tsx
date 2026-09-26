import { AlertCircle } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Extension } from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { ExtensionArtwork } from "@/shared/ui/ExtensionArtwork";
import { ListGroupRow } from "@/shared/ui/ListGroup";
import { ExtensionCardFooter } from "./ExtensionCardFooter";
import { useExtensionCardLabels } from "./useExtensionCardLabels";

export interface ExtensionCardProps {
  extension: Extension;
  /** Another write operation is running for this tool; the whole card is disabled. */
  busy?: boolean;
  /** The write operation currently running belongs to this card. */
  pending?: boolean;
  pendingLabel?: string;
  /** Position in the current filtered list, used to disambiguate same names. */
  position?: number;
  total?: number;
  /**
   * Which tools this local extension is also present in, already resolved to
   * display names. The page layer computes this from the local inventory and
   * passes it down — the card doesn't know about ToolId. Omit it when the
   * inventory hasn't arrived yet or failed to load, falling back to the
   * neutral description.
   */
  presentIn?: readonly string[];
  failure?: ExtensionToggleFailure;
  detectedResourceAction?: "browse" | "edit";
  detectedResourceError?: Error;
  updateAvailable?: boolean;
  onOpenLocation?: () => void;
  onEditDetectedDocument?: () => void;
  onEdit?: () => void;
  onRemove?: () => void;
  onUpdate?: () => void;
  onToggle: (enabled: boolean) => void;
}

export interface ExtensionToggleFailure {
  error: Error;
  intendedEnabled: boolean;
}

/**
 * Spec §36: an item shows only a name, one human-readable description, and one
 * control. It is a row of a `ListGroup` (ADR-0052) — no command, no arguments, no path. Everything displayable comes
 * from `Extension`, and that model has no field for a raw config payload, so
 * "don't show raw config on first entry" isn't a rule we're following — it's
 * structural.
 */
export function ExtensionCard({
  extension,
  busy = false,
  pending = false,
  pendingLabel,
  position = 1,
  total = 1,
  presentIn,
  failure,
  detectedResourceAction,
  detectedResourceError,
  updateAvailable = false,
  onOpenLocation,
  onEditDetectedDocument,
  onEdit,
  onRemove,
  onUpdate,
  onToggle,
}: ExtensionCardProps) {
  const { t } = useTranslation();
  const headingId = useId();
  const descriptionId = useId();
  const descriptionRef = useRef<HTMLParagraphElement>(null);
  const [descriptionExpanded, setDescriptionExpanded] = useState(false);
  const [descriptionOverflows, setDescriptionOverflows] = useState(false);
  const description =
    extension.description ?? t("extensions.card.noDescription");
  const labels = useExtensionCardLabels(
    extension,
    position,
    total,
    failure,
    detectedResourceError,
    presentIn,
  );
  const shownError = failure?.error ?? detectedResourceError;
  const failureCopy = shownError ? toErrorCopy(shownError) : null;
  const detected = extension.management === "detected";

  useEffect(() => {
    const node = descriptionRef.current;
    // Once expanded, the clamp is off and the measurement is guaranteed not
    // to overflow. That measurement would be meaningless, and it would also
    // make the "collapse" button collapse itself — so we only measure in the
    // collapsed state and keep the previous conclusion while expanded.
    if (node === null || descriptionExpanded) return;
    const measure = () =>
      setDescriptionOverflows(node.scrollHeight > node.clientHeight);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    return () => observer.disconnect();
  }, [description, descriptionExpanded]);

  return (
    <ListGroupRow
      role="article"
      aria-labelledby={headingId}
      aria-busy={pending || detectedResourceAction !== undefined}
      className={cn(
        "group isolate grid grid-cols-[auto_minmax(0,1fr)] items-start gap-x-3 gap-y-2 md:grid-cols-[auto_minmax(0,1fr)_auto]",
        "transition-colors duration-fast ease-standard focus-within:bg-layer-1",
        shownError && "bg-danger/5",
      )}
    >
      {/* The on/off word in the actions says it; the bar only lets the eye
          find the enabled item first when scanning down the list. */}
      {extension.enabled ? (
        <span
          aria-hidden="true"
          className="pointer-events-none absolute inset-y-2 left-0 w-0.5 rounded-full bg-success"
        />
      ) : null}

      <ExtensionArtwork
        kind={extension.kind}
        active={extension.enabled}
        className="h-9 w-9 rounded-lg md:row-span-2"
      />
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-1">
          <h3
            id={headingId}
            className="break-words text-body font-medium text-content"
          >
            {extension.name}
          </h3>
          {detected ? (
            <Badge tone="neutral">{t("extensions.card.detected")}</Badge>
          ) : null}
          {updateAvailable ? (
            <Badge tone="warning">{t("extensions.card.updateAvailable")}</Badge>
          ) : null}
        </div>
        <p
          id={descriptionId}
          ref={descriptionRef}
          className={cn(
            "mt-0.5 break-words [overflow-wrap:anywhere] text-caption text-content-muted",
            !descriptionExpanded && "line-clamp-1",
          )}
        >
          {description}
        </p>
        {descriptionOverflows ? (
          <Button
            className="-ml-2.5"
            size="xs"
            variant="ghost"
            aria-controls={descriptionId}
            aria-expanded={descriptionExpanded}
            onClick={() => setDescriptionExpanded((value) => !value)}
          >
            {t(
              descriptionExpanded
                ? "extensions.card.collapseDescription"
                : "extensions.card.expandDescription",
            )}
          </Button>
        ) : null}
      </div>

      <ExtensionCardFooter
        extension={extension}
        detected={detected}
        busy={busy}
        pending={pending}
        pendingLabel={pendingLabel}
        failed={failure !== undefined}
        detectedResourceAction={detectedResourceAction}
        updateAvailable={updateAvailable}
        labels={labels}
        onOpenLocation={onOpenLocation}
        onEditDetectedDocument={onEditDetectedDocument}
        onEdit={onEdit}
        onRemove={onRemove}
        onUpdate={onUpdate}
        onToggle={onToggle}
      />
      {failureCopy && labels.failureTitle ? (
        <div
          role="alert"
          aria-label={labels.failureTitle}
          className="col-start-2 flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3 md:col-span-2"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-caption font-medium text-content">
              {labels.failureTitle}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {t(failureCopy.messageKey)}
            </p>
            {failureCopy.remediationKey ? (
              <p className="mt-0.5 text-caption text-content-muted">
                {t(failureCopy.remediationKey)}
              </p>
            ) : null}
            {failure ? (
              <p className="mt-0.5 text-caption text-content-muted">
                {t("extensions.card.retryHint")}
              </p>
            ) : null}
          </div>
        </div>
      ) : null}
    </ListGroupRow>
  );
}
