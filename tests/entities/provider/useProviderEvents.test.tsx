import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { providerKeys, useProviderEvents } from "@/entities/provider";
import { PROVIDER_CHANGED_EVENT } from "@/native";
import { emitTauriEvent } from "../../msw/tauriMocks";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

describe("useProviderEvents", () => {
  it("invalidates only the tool scope named by a tray event", async () => {
    const client = createTestQueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    renderHook(() => useProviderEvents(), { wrapper: withQueryClient(client) });

    emitTauriEvent(PROVIDER_CHANGED_EVENT, { tool: "codex" });
    await waitFor(() =>
      expect(invalidate).toHaveBeenCalledWith({
        queryKey: providerKeys.list("codex"),
      }),
    );
    expect(invalidate).toHaveBeenCalledWith({
      queryKey: providerKeys.runtimeContext("codex"),
    });
  });

  it("drops malformed tool identities", async () => {
    const client = createTestQueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => {});
    renderHook(() => useProviderEvents(), { wrapper: withQueryClient(client) });

    emitTauriEvent(PROVIDER_CHANGED_EVENT, { tool: "claude" });
    await waitFor(() => expect(consoleError).toHaveBeenCalled());
    expect(invalidate).not.toHaveBeenCalled();
    consoleError.mockRestore();
  });
});
