import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  openClawWorkspaceKeys,
  useOpenClawWorkspaceDocument,
  useOpenClawWorkspaceOverview,
} from "@/entities/openclaw-workspace";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

describe("OpenClaw workspace queries", () => {
  it("keeps the overview for the whole desktop session", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_overview`, () => {
        calls += 1;
        return HttpResponse.json({
          files: [],
          existingFiles: 0,
          dailyMemoryCount: 0,
          dailyMemoryBytes: 0,
          totalBytes: 0,
          limited: false,
        });
      }),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const first = renderHook(() => useOpenClawWorkspaceOverview(), { wrapper });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();
    const second = renderHook(() => useOpenClawWorkspaceOverview(), {
      wrapper,
    });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));

    expect(calls).toBe(1);
    expect(client.getQueryData(openClawWorkspaceKeys.overview)).toMatchObject({
      existingFiles: 0,
    });
  });

  it("does not read a document until the user selects a fixed file id", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_document`, () => {
        calls += 1;
        return HttpResponse.json({
          id: "memory",
          filename: "MEMORY.md",
          exists: false,
          content: "",
          sizeBytes: 0,
          modifiedAt: null,
        });
      }),
    );
    const { result, rerender } = renderHook(
      ({ file }: { file: "memory" | null }) =>
        useOpenClawWorkspaceDocument(file),
      {
        initialProps: { file: null } as { file: "memory" | null },
        wrapper: withQueryClient(createTestQueryClient()),
      },
    );
    expect(result.current.fetchStatus).toBe("idle");
    expect(calls).toBe(0);
    rerender({ file: "memory" });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(calls).toBe(1);
  });
});
