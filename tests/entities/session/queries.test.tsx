import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  sessionKeys,
  useResumeSession,
  useSessions,
  useSessionThread,
} from "@/entities/session";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const reference = "b".repeat(64);

describe("session queries", () => {
  it("keeps each search and tool scope in a separate cache entry", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, () =>
        HttpResponse.json({ items: [], totalCount: 0, limited: false }),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useSessions("release", "codex"), {
      wrapper: withQueryClient(client),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(sessionKeys.list("release", "codex"))).toEqual({
      items: [],
      totalCount: 0,
      limited: false,
    });
  });

  it("does not read message content until a reference is selected", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_thread`, () => {
        calls += 1;
        return HttpResponse.json({
          reference,
          messages: [],
          totalCount: 0,
          limited: false,
          workingDirectory: null,
          resumeCommand: null,
        });
      }),
    );
    const client = createTestQueryClient();
    const { result, rerender } = renderHook(
      ({ selected }: { selected: string | null }) => useSessionThread(selected),
      {
        initialProps: { selected: null } as { selected: string | null },
        wrapper: withQueryClient(client),
      },
    );
    expect(result.current.fetchStatus).toBe("idle");
    expect(calls).toBe(0);

    rerender({ selected: reference });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(calls).toBe(1);
  });

  it("hands resume to the dedicated mutation", async () => {
    let payload: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_resume`, async ({ request }) => {
        payload = await request.json();
        return HttpResponse.json(null);
      }),
    );
    const { result } = renderHook(() => useResumeSession(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate(reference);

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(payload).toEqual({ reference });
  });
});
