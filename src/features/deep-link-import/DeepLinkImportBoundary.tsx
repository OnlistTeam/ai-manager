import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  useConfirmDeepLink,
  useDeepLinkEvents,
  useDeepLinkPending,
  useDismissDeepLink,
} from "@/entities/deeplink";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { DeepLinkConfirmationModal } from "./DeepLinkConfirmationModal";

/**
 * The one place a waiting link turns into a dialog (ADR-0029).
 *
 * The queue is native state, not a navigation intent: an ordinary navigation
 * must not clear it, and the dialog has to be reachable from whatever page the
 * user happens to be on. So this lives next to the app shell and reads the
 * queue directly, rather than going through the one-shot route intents.
 */
export function DeepLinkImportBoundary() {
  const { t } = useTranslation();
  useDeepLinkEvents();
  const pending = useDeepLinkPending();
  const confirm = useConfirmDeepLink();
  const dismiss = useDismissDeepLink();
  const [closing, setClosing] = useState<string | null>(null);

  // The oldest waiting link is answered first; the rest reappear behind it.
  const current = pending.data?.[0] ?? null;

  useEffect(() => {
    if (current && closing && current.id !== closing) setClosing(null);
  }, [current, closing]);

  if (!current || closing === current.id) return null;

  const error = confirm.isError ? toErrorCopy(confirm.error) : null;

  function close(open: boolean) {
    if (open || !current || confirm.isPending) return;
    setClosing(current.id);
    confirm.reset();
    dismiss.mutate(current.id);
  }

  return (
    <DeepLinkConfirmationModal
      open
      preview={current}
      busy={confirm.isPending}
      error={error}
      onOpenChange={close}
      onConfirm={() => {
        if (confirm.isPending) return;
        confirm.mutate(current.id, {
          onSuccess: (outcome) => {
            confirm.reset();
            toast.success(
              t("deeplink.toast.imported", { count: outcome.applied }),
            );
          },
        });
      }}
    />
  );
}
