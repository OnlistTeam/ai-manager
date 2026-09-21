import {
  AlertTriangle,
  Globe2,
  Pencil,
  Radio,
  RefreshCw,
  ShieldCheck,
  SlidersHorizontal,
  Trash2,
} from "lucide-react";
import { useId, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type {
  EffectiveCredential,
  Provider,
  ProviderEffectiveState,
  ProviderTestResult,
} from "@/entities/provider";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { CopyButton } from "@/shared/ui/CopyButton";
import { ServiceArtwork } from "@/shared/ui/ServiceArtwork";
import { ServiceCard } from "@/shared/ui/ServiceCard";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/shared/ui/Tooltip";
import { ProviderActionError } from "./ProviderActionError";
import { ProviderCheckOutcome } from "./ProviderCheckOutcome";
import { providerKeyPresentation } from "./providerKeyPresentation";

export interface ProviderCardProps {
  provider: Provider;
  /** Display name of the tool that owns this provider (the "Currently Used by" in §35). */
  toolName: string;
  /** Three states: currently in effect / selected but overridden / just saved. */
  effectiveState: ProviderEffectiveState;
  effectiveCredential?: EffectiveCredential;
  /** The variable name or file path that overrode this selection; `null` when `overridden` and the source is unknown. */
  overrideSource?: string | null;
  /** A one-line explanation of "why this one" while it's in effect (only needed when the source is a shell/environment variable). */
  sourceNote?: string | null;
  /** Opens the config file this tool actually reads; only the card that's currently in effect gets this. */
  configAction?: ReactNode;
  busy?: boolean;
  testing?: boolean;
  switching?: boolean;
  tryingNext?: boolean;
  tryNextUnavailable?: boolean;
  switchError?: Error;
  testError?: Error;
  testResult?: ProviderTestResult;
  onUse?: () => void;
  onEdit?: () => void;
  onTest?: () => void;
  onTryNext?: () => void;
  onBrowseCompatible?: () => void;
  onRemove?: () => void;
}

interface RemoveActionProps {
  name: string;
  /** The card is busy; no action should respond. */
  blocked: boolean;
  /** The reason removal isn't allowed; `null` means removal is allowed. */
  hint: string | null;
  hintId: string;
  onRemove?: () => void;
}

/**
 * Remove button. When removal isn't allowed, the reason no longer sits
 * permanently on the card — that line is just noise for most people, and
 * only the person who actually clicks it needs it. So the reason lives in a
 * hover tooltip, with an sr-only explanation attached via the button's
 * `aria-describedby`: screen reader users won't "hover", so that
 * explanation is their entry point.
 */
function RemoveAction({
  name,
  blocked,
  hint,
  hintId,
  onRemove,
}: RemoveActionProps) {
  const { t } = useTranslation();
  const button = (
    <Button
      variant="ghost"
      disabled={blocked || hint !== null}
      aria-describedby={hint ? hintId : undefined}
      aria-label={t("services.action.removeNamed", { name })}
      onClick={onRemove}
    >
      <Trash2 className="h-4 w-4" aria-hidden="true" />
      {t("services.action.remove")}
    </Button>
  );

  if (hint === null) return button;

  return (
    <TooltipProvider delayDuration={200} skipDelayDuration={100}>
      <Tooltip>
        {/* A disabled button doesn't fire pointer events, so the trigger wraps it instead of being it. */}
        <TooltipTrigger asChild>
          <span className="flex">{button}</span>
        </TooltipTrigger>
        <TooltipContent side="top">{hint}</TooltipContent>
      </Tooltip>
      <p id={hintId} className="sr-only">
        {hint}
      </p>
    </TooltipProvider>
  );
}

export function ProviderCard({
  provider,
  toolName,
  effectiveState,
  effectiveCredential,
  overrideSource = null,
  sourceNote = null,
  configAction = null,
  busy = false,
  testing = false,
  switching = false,
  tryingNext = false,
  tryNextUnavailable = false,
  switchError,
  testError,
  testResult,
  onUse,
  onEdit,
  onTest,
  onTryNext,
  onBrowseCompatible,
  onRemove,
}: ProviderCardProps) {
  const { t } = useTranslation();
  const removeHintId = useId();
  const switchTitle = switchError
    ? t("services.switch.errorNamed", { name: provider.name })
    : undefined;
  const testTitle = testError
    ? t("services.test.errorNamed", { name: provider.name })
    : undefined;
  const cardBusy = switching || testing || tryingNext;
  const actionsBlocked = busy || cardBusy;
  const useActionState = switching ? "pending" : switchError ? "retry" : "idle";
  const useAriaLabel = switching
    ? t("services.switch.switchingNamed", { name: provider.name })
    : switchError
      ? t("services.switch.retryNamed", { name: provider.name })
      : t(
          provider.additive
            ? "services.action.configureNamed"
            : "services.action.useNamed",
          { name: provider.name },
        );
  const testAriaLabel = testing
    ? t("services.test.checkingNamed", { name: provider.name })
    : testError
      ? t("services.test.retryNamed", { name: provider.name })
      : t("services.action.testNamed", { name: provider.name });
  const KindIcon =
    provider.kind === "official" ? ShieldCheck : SlidersHorizontal;
  const keyPresentation = providerKeyPresentation(
    provider,
    toolName,
    t,
    effectiveCredential,
  );
  const KeyIcon = keyPresentation.icon;
  const removeHint = provider.canRemove
    ? null
    : t(
        provider.active
          ? "services.remove.activeHint"
          : "services.remove.managedHint",
      );

  return (
    <ServiceCard
      name={provider.name}
      icon={
        <ServiceArtwork provider={provider} className="h-10 w-10 rounded-lg" />
      }
      usedBy={effectiveState === "inUse" ? [toolName] : []}
      connected
      active={effectiveState === "inUse"}
      useAvailable={!(provider.active && effectiveState === "inUse")}
      activeLabelKey="services.card.inUse"
      notUsedLabelKey={`services.card.${effectiveState === "overridden" ? "saved" : effectiveState}`}
      useLabelKey={provider.additive ? "ds.action.configure" : undefined}
      busy={cardBusy}
      actionDisabled={busy}
      useActionState={useActionState}
      useAriaLabel={useAriaLabel}
      onUse={onUse}
      compact
      className={
        switchError || testError ? "border-danger/30 shadow-sm" : undefined
      }
      meta={
        <>
          <Badge
            tone={provider.kind === "official" ? "brand" : "neutral"}
            icon={KindIcon}
          >
            {t(`services.card.${provider.kind}`)}
          </Badge>
          {effectiveState === "overridden" ? (
            <Badge tone="warning" icon={AlertTriangle}>
              {overrideSource
                ? t("services.card.overriddenBy", { source: overrideSource })
                : t("services.card.notInEffect")}
            </Badge>
          ) : null}
          {effectiveState !== "inUse" && effectiveState !== "overridden" ? (
            <Badge tone="neutral">{t(`services.card.${effectiveState}`)}</Badge>
          ) : null}
        </>
      }
      detail={
        <div className="flex flex-col gap-2.5">
          {provider.baseUrl ? (
            <p className="flex min-w-0 items-start gap-2 text-content-muted">
              <Globe2
                className="mt-0.5 h-4 w-4 shrink-0 text-brand"
                aria-hidden="true"
              />
              <span className="min-w-0 flex-1">
                <span className="sr-only">
                  {t("services.card.savedEndpoint")}
                </span>
                <span className="block break-all font-mono text-mono-sm text-content">
                  {provider.baseUrl}
                </span>
              </span>
              <CopyButton
                value={provider.baseUrl}
                label={t("services.card.savedEndpoint")}
                className="-my-1"
              />
            </p>
          ) : null}
          {sourceNote ? (
            <p className="text-caption text-content-muted">{sourceNote}</p>
          ) : null}
          <p className="flex min-w-0 items-start gap-2 text-content-muted">
            <KeyIcon
              className={`mt-0.5 h-4 w-4 shrink-0 ${keyPresentation.iconClassName}`}
              aria-hidden="true"
            />
            <span
              className={`min-w-0 flex-1 break-all ${keyPresentation.textClassName}`}
            >
              {keyPresentation.text}
            </span>
            {keyPresentation.copyValue ? (
              <CopyButton
                value={keyPresentation.copyValue}
                label={t("services.form.key")}
                className="-my-1"
              />
            ) : null}
          </p>
          {testResult && !testError ? (
            <ProviderCheckOutcome
              result={testResult}
              providerName={provider.name}
              actionsBlocked={actionsBlocked}
              tryingNext={tryingNext}
              tryNextUnavailable={tryNextUnavailable}
              onTryNext={onTryNext}
              onBrowseCompatible={onBrowseCompatible}
            />
          ) : null}
          {testError && testTitle ? (
            <ProviderActionError
              title={testTitle}
              error={testError}
              retryHint={t("services.test.retryHint")}
            />
          ) : null}
          {switchError && switchTitle ? (
            <ProviderActionError
              title={switchTitle}
              error={switchError}
              retryHint={t("services.switch.retryHint")}
            />
          ) : null}
        </div>
      }
      actions={
        <>
          {provider.testable ? (
            <Button
              variant={testError ? "secondary" : "ghost"}
              disabled={actionsBlocked}
              loading={testing}
              aria-label={testAriaLabel}
              onClick={onTest}
            >
              {!testing ? (
                testError ? (
                  <RefreshCw className="h-4 w-4" aria-hidden="true" />
                ) : (
                  <Radio className="h-4 w-4" aria-hidden="true" />
                )
              ) : null}
              {t(testError ? "services.test.retry" : "services.action.test")}
            </Button>
          ) : null}
          <Button
            variant="ghost"
            disabled={actionsBlocked}
            aria-label={t("services.action.editNamed", {
              name: provider.name,
            })}
            onClick={onEdit}
          >
            <Pencil className="h-4 w-4" aria-hidden="true" />
            {t("services.action.edit")}
          </Button>
          {configAction}
          <RemoveAction
            name={provider.name}
            blocked={actionsBlocked}
            hint={removeHint}
            hintId={removeHintId}
            onRemove={onRemove}
          />
        </>
      }
    />
  );
}
