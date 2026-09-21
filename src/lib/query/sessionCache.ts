/**
 * Local inventories are snapshots of this desktop session, not web feeds.
 * Keep them until a user refreshes or a successful mutation invalidates them;
 * remounting a page must never start another disk/process scan by itself.
 */
export const sessionCacheOptions = {
  staleTime: Number.POSITIVE_INFINITY,
  gcTime: Number.POSITIVE_INFINITY,
  refetchOnMount: false,
  refetchOnWindowFocus: false,
  refetchOnReconnect: false,
} as const;
