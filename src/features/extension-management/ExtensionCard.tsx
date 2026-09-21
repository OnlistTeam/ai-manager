import { AlertCircle } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Extension } from "@/entities/extension";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { cn } from "@/shared/ui/cn";
import { ExtensionArtwork } from "@/shared/ui/ExtensionArtwork";
import { ExtensionCardFooter } from "./ExtensionCardFooter";
import { useExtensionCardLabels } from "./useExtensionCardLabels";

const KIND_TITLE_KEY: Record<Extension["kind"], string> = {
  skill: "extensions.skill.title",
  mcp: "extensions.mcp.title",
  prompt: "extensions.prompt.title",
};

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
  onCopyToTool?: () => void;
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
 * Spec §36: a card shows only a name, one human-readable description, and one
 * control — no command, no arguments, no path. Everything displayable comes
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
  onCopyToTool,
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
    <Card
      padding="none"
      role="article"
      aria-labelledby={headingId}
      aria-busy={pending || detectedResourceAction !== undefined}
      className={cn(
        "group relative isolate flex min-h-56 flex-col overflow-hidden rounded-xl border-hairline bg-layer-1 transition-[border-color,box-shadow] duration-fast ease-standard focus-within:border-brand/25 focus-within:shadow-md",
        extension.enabled && "border-brand/25 shadow-md",
        shownError && "border-danger/30 shadow-sm",
      )}
    >
      <span
        aria-hidden="true"
        className={cn(
          "pointer-events-none absolute -right-10 -top-12 h-36 w-36 rounded-full blur-2xl",
          extension.enabled ? "bg-success/10" : "bg-brand/5",
        )}
      />

      <div className="relative flex items-start gap-4 p-5 pb-4">
        <ExtensionArtwork kind={extension.kind} active={extension.enabled} />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <Badge tone={extension.enabled ? "brand" : "neutral"}>
              {t(KIND_TITLE_KEY[extension.kind])}
            </Badge>
            {detected ? (
              <Badge tone="neutral">{t("extensions.card.detected")}</Badge>
            ) : null}
            {updateAvailable ? (
              <Badge tone="warning">
                {t("extensions.card.updateAvailable")}
              </Badge>
            ) : null}
          </div>
          <h3
            id={headingId}
            className="mt-2 break-words text-heading text-content"
          >
            {extension.name}
          </h3>
          <p
            id={descriptionId}
            ref={descriptionRef}
            className={cn(
              "mt-2 break-words [overflow-wrap:anywhere] text-body text-content-muted",
              !descriptionExpanded && "line-clamp-2",
            )}
          >
            {description}
          </p>
          {descriptionOverflows ? (
            <Button
              className="-ml-2.5 mt-1"
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
      </div>

      {failureCopy && labels.failureTitle ? (
        <div
          role="alert"
          aria-label={labels.failureTitle}
          className="relative mx-5 mb-4 flex items-start gap-2 rounded-md border border-danger/30 bg-danger/5 p-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-caption font-medium text-content">
              {labels.failureTitle}
            </p>
            <p className="mt-0.5 text-caption leading-5 text-content-muted">
              {t(failureCopy.messageKey)}
            </p>
            {failureCopy.remediationKey ? (
              <p className="mt-0.5 text-caption leading-5 text-content-muted">
                {t(failureCopy.remediationKey)}
              </p>
            ) : null}
            {failure ? (
              <p className="mt-0.5 text-caption leading-5 text-content-muted">
                {t("extensions.card.retryHint")}
              </p>
            ) : null}
          </div>
        </div>
      ) : null}

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
        onCopyToTool={onCopyToTool}
        onEdit={onEdit}
        onRemove={onRemove}
        onUpdate={onUpdate}
        onToggle={onToggle}
      />
    </Card>
  );
}
