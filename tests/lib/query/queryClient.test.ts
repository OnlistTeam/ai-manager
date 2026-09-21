import { describe, expect, it } from "vitest";
import { queryClient } from "@/lib/query/queryClient";

describe("production QueryClient", () => {
  it("keeps local snapshots for the process instead of refetching on navigation", () => {
    expect(queryClient.getDefaultOptions().queries).toMatchObject({
      staleTime: Number.POSITIVE_INFINITY,
      gcTime: Number.POSITIVE_INFINITY,
      refetchOnMount: false,
      refetchOnWindowFocus: false,
      refetchOnReconnect: false,
    });
  });
});
