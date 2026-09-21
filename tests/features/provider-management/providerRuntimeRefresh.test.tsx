import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { providerKeys } from "@/entities/provider";
import { useProviderActivationPreflight } from "@/features/provider-management";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

describe("provider runtime refresh", () => {
  it.each([false, true])(
    "invalidates runtime evidence after activation (failed=%s)",
    async (failed) => {
      server.use(
        http.post("http://tauri.local/app_provider_activation_prepare", () =>
          failed
            ? HttpResponse.json(
                {
                  code: "CONFIG_WRITE_FAILED",
                  messageKey: "error.unknown",
                  technicalMessage: null,
                  remediation: null,
                  contextId: null,
                },
                { status: 500 },
              )
            : HttpResponse.json({
                status: "notChecked",
                originProviderId: null,
                activeProviderId: null,
                providers: [],
                checks: [],
              }),
        ),
      );
      const client = createTestQueryClient();
      const invalidate = vi.spyOn(client, "invalidateQueries");
      const { result } = renderHook(() => useProviderActivationPreflight(), {
        wrapper: withQueryClient(client),
      });
      act(() =>
        result.current.mutate({ tool: "opencode", providerId: "relay" }),
      );
      await waitFor(() =>
        expect(failed ? result.current.isError : result.current.isSuccess).toBe(
          true,
        ),
      );
      expect(invalidate).toHaveBeenCalledWith({
        queryKey: providerKeys.runtimeContext("opencode"),
      });
    },
  );
});
