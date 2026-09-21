import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import { settings } from "@/native/commands/settings";
import { NativeError } from "@/native/schemas/error";
import type { ProductSettings, TerminalAppId } from "@/native/schemas/settings";

export const settingsKeys = {
  all: ["settings"] as const,
  current: () => ["settings", "current"] as const,
  terminals: () => ["settings", "terminals"] as const,
  /** Mutation key of every product-settings write, so overlapping saves can see each other. */
  save: () => ["settings", "save"] as const,
};

/**
 * The backend does a full replace: when two saves run concurrently, the one
 * that finishes writing last wins, and the cache ends up on the snapshot
 * from the last one to **return** — the two are independently random, so
 * the store and the UI can end up showing two different values. Mutations
 * in the same scope are executed one at a time by TanStack in submission
 * order (the later one stays pending first), and each mutationFn re-merges
 * from a cache that already contains the previous result, so no save is
 * ever lost.
 */
const SAVE_SCOPE = { id: "product-settings" } as const;

/**
 * Spec §22: product settings are server state, so they only go through
 * Query. `staleTime: Infinity` — only this process ever changes this data,
 * and once it does, the mutation writes straight back to the cache, so
 * there's never a reason to refetch.
 */
export function productSettingsQueryOptions() {
  return queryOptions({
    queryKey: settingsKeys.current(),
    queryFn: () => settings.get(),
    ...sessionCacheOptions,
    // After a read failure, recovery is explicit via the settings page's
    // "Retry" — if another page mounts the same query, an automatic retry
    // would push the no-data state back to pending, and AppRoot's initial
    // ready gate would white-screen the whole shell as a result.
    retryOnMount: false,
  });
}

export function useProductSettings(): UseQueryResult<ProductSettings, Error> {
  return useQuery(productSettingsQueryOptions());
}

/**
 * Which terminals are installed on this machine. Whether something is
 * installed is a fact about this machine that won't change within a
 * session, so — like other local inventories — reading it once is enough.
 */
export function useAvailableTerminals(): UseQueryResult<
  TerminalAppId[],
  Error
> {
  return useQuery({
    queryKey: settingsKeys.terminals(),
    queryFn: () => settings.availableTerminals(),
    ...sessionCacheOptions,
  });
}

/**
 * The **only place in the whole project** that merges a `Partial` into a
 * full object (decision 3).
 *
 * Optimistic write: tabs and toggles must move the instant they're clicked —
 * waiting for a round trip first would make the user think the click didn't
 * register. On failure, put back the pre-change snapshot — a toggle stuck
 * at where the user clicked while the store holds a different value is
 * worse than the toggle just bouncing back on its own.
 */
export interface SaveProductSettingsContext {
  previous?: ProductSettings;
  /** Other saves were already queued ahead of this one at submit time: `previous` includes their optimistic values. */
  queued: boolean;
}

export function useSaveProductSettings(): UseMutationResult<
  ProductSettings,
  Error,
  Partial<ProductSettings>,
  SaveProductSettingsContext
> {
  const queryClient = useQueryClient();
  // While a lifecycle callback is running, this mutation itself still counts as pending, so "1" means "just me".
  const pendingSaves = () =>
    queryClient.isMutating({ mutationKey: settingsKeys.save() });

  return useMutation<
    ProductSettings,
    Error,
    Partial<ProductSettings>,
    SaveProductSettingsContext
  >({
    mutationKey: settingsKeys.save(),
    scope: SAVE_SCOPE,
    // `onMutate` has already written the merged result into the cache, so
    // what's read here is exactly what's about to be sent. When queued
    // behind another save this step actually matters: by the time this one
    // runs, the cache has already been replaced by the previous result, so
    // the patch has to be re-merged on top of it.
    //
    // **Never write without a baseline.** The backend does a full replace,
    // so using defaults as the baseline would backfill any fields we
    // couldn't read with defaults — a stray tab click right after a failed
    // settings read would wipe out the user's real `importPromptSeen` /
    // `advancedMode` in the store, so the startup import question would be
    // asked again on the next launch. When settings can't be read, both
    // tabbed pages already have their own local fallback, so the UI still
    // works — it just doesn't persist.
    mutationFn: (patch) => {
      const current = queryClient.getQueryData<ProductSettings>(
        settingsKeys.current(),
      );
      if (!current) {
        return Promise.reject(
          new NativeError({
            code: "INTERNAL",
            messageKey: "error.settings.saveFailed",
            technicalMessage: "no settings snapshot to merge onto",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
        );
      }
      return settings.save({ ...current, ...patch });
    },
    onMutate: async (patch) => {
      await queryClient.cancelQueries({ queryKey: settingsKeys.current() });
      const previous = queryClient.getQueryData<ProductSettings>(
        settingsKeys.current(),
      );
      // Don't write an optimistic value when the cache is empty: that would
      // disguise a failed-read query as a success, making the error-state
      // UI disappear, and `staleTime: Infinity` means it would never self-heal.
      if (previous) {
        queryClient.setQueryData(settingsKeys.current(), {
          ...previous,
          ...patch,
        });
      }
      return { previous, queued: pendingSaves() > 1 };
    },
    onError: (_error, _patch, context) => {
      // Write back unconditionally. When `previous` is undefined, this step
      // evicts the entry from the cache, letting the query return to its
      // true (failed) state instead of getting stuck on an optimistic value.
      queryClient.setQueryData(settingsKeys.current(), context?.previous);
      // The `previous` snapshot captured while queued may include the
      // optimistic value from an earlier save, which might itself have
      // already failed and rolled back. When the queue drains and ends in
      // failure, re-read the real value from the store.
      if (context?.queued && pendingSaves() === 1) {
        void queryClient.invalidateQueries({
          queryKey: settingsKeys.current(),
        });
      }
    },
    onSuccess: (saved) => {
      // The backend returns the **read-back** result — write it in directly
      // as the authoritative value, no extra round trip needed. Skip
      // writing when there's another save still queued behind this one: it
      // will resend carrying this result and write back its own read-back
      // value, so writing now would just make the UI regress to the old
      // value before jumping to the new one.
      if (pendingSaves() > 1) return;
      queryClient.setQueryData(settingsKeys.current(), saved);
    },
  });
}
