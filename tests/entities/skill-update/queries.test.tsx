import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { skillUpdateKeys, useSkillUpdates } from "@/entities/skill-update";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

describe("useSkillUpdates", () => {
  it("checks once and reuses the result for the desktop session", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_updates_check`, () => {
        calls += 1;
        return HttpResponse.json([{ id: "repo:review", name: "Review" }]);
      }),
    );
    const client = createTestQueryClient();
    const first = renderHook(() => useSkillUpdates(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();

    const second = renderHook(() => useSkillUpdates(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(second.result.current.data?.[0]?.name).toBe("Review");
    expect(client.getQueryData(skillUpdateKeys.list())).toHaveLength(1);
    expect(calls).toBe(1);
  });

  it("does not check before the Skills surface is enabled", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_updates_check`, () => {
        calls += 1;
        return HttpResponse.json([]);
      }),
    );
    const { result } = renderHook(() => useSkillUpdates(false), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.fetchStatus).toBe("idle"));
    expect(calls).toBe(0);
  });
});
