import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  skillRepositoryKeys,
  useSkillRepositories,
} from "@/entities/skill-repository";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

describe("useSkillRepositories", () => {
  it("reads once and reuses the source list for the desktop session", async () => {
    let calls = 0;
    const repository = {
      id: "a".repeat(64),
      owner: "anthropics",
      repository: "skills",
      branch: "main",
      enabled: true,
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_repositories_list`, () => {
        calls += 1;
        return HttpResponse.json([repository]);
      }),
    );
    const client = createTestQueryClient();
    const first = renderHook(() => useSkillRepositories(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();

    const second = renderHook(() => useSkillRepositories(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(second.result.current.data).toEqual([repository]);
    expect(client.getQueryData(skillRepositoryKeys.list())).toEqual([
      repository,
    ]);
    expect(calls).toBe(1);
  });

  it("does not read before the source surface is enabled", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_repositories_list`, () => {
        calls += 1;
        return HttpResponse.json([]);
      }),
    );
    const { result } = renderHook(() => useSkillRepositories(false), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.fetchStatus).toBe("idle"));
    expect(calls).toBe(0);
  });
});
