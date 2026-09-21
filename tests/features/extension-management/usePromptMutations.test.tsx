import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import { extensionKeys } from "@/entities/extension";
import {
  useImportPrompt,
  useRemovePrompt,
  useSavePrompt,
} from "@/features/extension-management";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

/**
 * The page reads the scoped key. If the write-back uses a bare ToolId, the save
 * succeeds and the cache does get written, just to a key nobody subscribes to —
 * the user finishes creating a prompt and the list doesn't budge.
 */
const LIST_KEY = extensionKeys.list("tool:claude-code", "prompt");

const wire = {
  kind: "prompt",
  id: "prompt-1",
  scope: { kind: "tool", id: "claude-code" },
  name: "Team defaults",
  description: null,
  management: "managed",
  enabled: false,
  canDisable: false,
};

function mountSave(client: QueryClient) {
  return renderHook(() => useSavePrompt(), {
    wrapper: withQueryClient(client),
  });
}

describe("usePromptMutations", () => {
  it("puts a newly created prompt on the list the page is actually reading", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_save`, () =>
        HttpResponse.json([wire]),
      ),
    );
    const client = createTestQueryClient();
    const { result } = mountSave(client);

    result.current.mutate({
      tool: "claude-code",
      promptId: null,
      draft: { name: "Team defaults", description: null, content: "Be brief." },
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(LIST_KEY)).toEqual([wire]);
  });

  it("refreshes that same list when a save fails", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_save`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.prompt.saveFailed",
            technicalMessage: "permission denied at ~/.claude/CLAUDE.md",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = mountSave(client);

    result.current.mutate({
      tool: "claude-code",
      promptId: null,
      draft: { name: "Team defaults", description: null, content: "Be brief." },
    });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: LIST_KEY });
  });

  it("writes the same key after removing and after importing the live file", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_remove`, () =>
        HttpResponse.json([]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_prompt_import_current`, () =>
        HttpResponse.json([wire]),
      ),
    );
    const client = createTestQueryClient();

    const remove = renderHook(() => useRemovePrompt(), {
      wrapper: withQueryClient(client),
    });
    remove.result.current.mutate({ tool: "claude-code", promptId: "prompt-1" });
    await waitFor(() => expect(remove.result.current.isSuccess).toBe(true));
    expect(client.getQueryData(LIST_KEY)).toEqual([]);

    const importCurrent = renderHook(() => useImportPrompt(), {
      wrapper: withQueryClient(client),
    });
    importCurrent.result.current.mutate({ tool: "claude-code" });
    await waitFor(() =>
      expect(importCurrent.result.current.isSuccess).toBe(true),
    );
    expect(client.getQueryData(LIST_KEY)).toEqual([wire]);
  });
});
