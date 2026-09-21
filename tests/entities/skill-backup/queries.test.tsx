import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { skillBackupKeys, useSkillBackups } from "@/entities/skill-backup";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

describe("useSkillBackups", () => {
  it("reads once and reuses the recovery-copy list for the desktop session", async () => {
    let calls = 0;
    const backup = {
      id: "b".repeat(64),
      name: "Review",
      description: null,
      createdAt: 1_787_689_200,
      conflicts: false,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_backups_list`, () => {
        calls += 1;
        return HttpResponse.json([backup]);
      }),
    );
    const client = createTestQueryClient();
    const first = renderHook(() => useSkillBackups(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();

    const second = renderHook(() => useSkillBackups(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(second.result.current.data).toEqual([backup]);
    expect(client.getQueryData(skillBackupKeys.list())).toEqual([backup]);
    expect(calls).toBe(1);
  });
});
