import { useEffect, useRef } from "react";
import { Loader2, Radio, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";

import { connectivityFor, useProviderConnectivity } from "@/entities/health";
import type { Provider } from "@/entities/provider";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { ProviderActionError } from "./ProviderActionError";
import { describeReachability } from "./testPresentation";
import { useTestProvider } from "./useProviderMutations";

interface ProviderProbeReachabilityProps {
  provider: Provider;
}

/**
 * The free, credential-free address check, kept as the dialog's first line.
 *
 * It runs once when the dialog opens and reuses the shared connectivity cache,
 * so the home health check and the failover flow see the same result. Its job
 * here is diagnostic: if this line fails, the model list and the probe below
 * were never going to work, and the reason is the address, not the key.
 */
export function ProviderProbeReachability({
  provider,
}: ProviderProbeReachabilityProps) {
  const { t } = useTranslation();
  const connectivity = useProviderConnectivity();
  const check = useTestProvider({ notifyOnError: false });
  const checked = useRef<string | null>(null);

  const result = connectivityFor(
    connectivity.data ?? {},
    provider.tool,
    provider.id,
  );

  useEffect(() => {
    if (!provider.testable) return;
    if (checked.current === provider.id) return;
    checked.current = provider.id;
    check.mutate({ tool: provider.tool, providerId: provider.id });
    // `check` is a stable mutation handle from React Query.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [provider.id, provider.tool, provider.testable]);

  if (!provider.testable) return null;

  // Checked before the pending line: a failed check leaves nothing in the
  // shared cache, so an error state also has no result to show.
  if (check.error) {
    return (
      <div className="flex flex-col items-start gap-2">
        <ProviderActionError
          title={t("services.test.errorNamed", { name: provider.name })}
          error={check.error}
          retryHint={t("services.test.retryHint")}
        />
        <Button
          variant="secondary"
          aria-label={t("services.test.retryNamed", { name: provider.name })}
          onClick={() =>
            check.mutate({ tool: provider.tool, providerId: provider.id })
          }
        >
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          {t("services.test.retry")}
        </Button>
      </div>
    );
  }

  /*
   * One pending line for two states that are the same thing to the user: the
   * request is in flight, or it has landed and the shared cache has not been
   * read back yet.
   *
   * The spinner is not decoration. The address check allows one retry on a
   * timeout, so a dead host can hold this line for up to sixteen seconds, and a
   * caption that never moves for that long reads as a hang.
   */
  if (check.isPending || result === undefined) {
    return (
      <p
        role="status"
        aria-live="polite"
        className="inline-flex items-center gap-2 text-caption text-content-muted"
      >
        <Loader2
          className="h-3.5 w-3.5 shrink-0 motion-safe:animate-spin"
          aria-hidden="true"
        />
        {t("services.probe.addressChecking")}
      </p>
    );
  }

  const outcome = describeReachability(result);
  return (
    <Badge tone={outcome.tone} icon={Radio} className="self-start">
      <span>{t(outcome.labelKey)}</span>
      {outcome.responseTimeMs === null ? null : (
        <span>
          {" "}
          · {t("services.test.latency", { ms: outcome.responseTimeMs })}
        </span>
      )}
    </Badge>
  );
}
