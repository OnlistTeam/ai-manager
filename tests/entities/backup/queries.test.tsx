import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  backupKeys,
  useBackupSchedule,
  useBackups,
  useSaveBackupSchedule,
  type BackupList,
} from "@/entities/backup";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const LIST: BackupList = {
  files: [
    {
      name: "db_backup_20260819_101500.db",
      createdAt: "2026-08-19T10:15:00+08:00",
      sizeBytes: 2_097_152,
    },
  ],
};

describe("useBackups", () => {
  it("caches the folder and the files under one key", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json(LIST),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useBackups(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(backupKeys.list())).toEqual(LIST);
  });

  it("surfaces a failed read instead of pretending there are no backups", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "UPSTREAM_ERROR",
            messageKey: "error.backup.listFailed",
            technicalMessage: null,
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const { result } = renderHook(() => useBackups(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.data).toBeUndefined();
  });
});

describe("backup schedule queries", () => {
  it("reads the policy under its own key", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_schedule_get`, () =>
        HttpResponse.json({ automatic: true, retainCount: 10 }),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useBackupSchedule(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(backupKeys.schedule())).toEqual({
      automatic: true,
      retainCount: 10,
    });
  });

  it("replaces the cached policy with the backend read-back", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_schedule_save`, () =>
        HttpResponse.json({ automatic: false, retainCount: 10 }),
      ),
    );
    const client = createTestQueryClient();
    client.setQueryData(backupKeys.schedule(), {
      automatic: true,
      retainCount: 10,
    });
    const { result } = renderHook(() => useSaveBackupSchedule(), {
      wrapper: withQueryClient(client),
    });
    result.current.mutate({ automatic: false, retainCount: 10 });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(client.getQueryData(backupKeys.schedule())).toEqual({
      automatic: false,
      retainCount: 10,
    });
  });
});
