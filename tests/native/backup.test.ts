import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native, type BackupList } from "@/native";
import { server } from "../msw/server";

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

describe("native.backup", () => {
  it("reads the files", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json(LIST),
      ),
    );
    await expect(native.backup.list()).resolves.toEqual(LIST);
  });

  it("never hands the renderer a backup folder path", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backups_list`, () =>
        HttpResponse.json({
          ...LIST,
          directory: "/Users/a/Library/Application Support/ai-manager/backups",
        }),
      ),
    );
    const list = await native.backup.list();
    expect(list).toEqual(LIST);
    expect(JSON.stringify(list)).not.toMatch(/directory|\/Users\//u);
  });

  it("hands back the refreshed list after a new backup", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_create`, () =>
        HttpResponse.json(LIST),
      ),
    );
    await expect(native.backup.create()).resolves.toEqual(LIST);
  });

  it("names the file under `backup` when restoring, and reports the sync flag", async () => {
    let received: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_restore`, async ({ request }) => {
        received = await request.json();
        return HttpResponse.json({ backups: LIST, toolsOutOfSync: true });
      }),
    );
    await expect(
      native.backup.restore("db_backup_20260819_101500.db"),
    ).resolves.toEqual({ backups: LIST, toolsOutOfSync: true });
    expect(received).toEqual({ backup: "db_backup_20260819_101500.db" });
  });

  it("names the file under `backup` when deleting", async () => {
    let received: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_delete`, async ({ request }) => {
        received = await request.json();
        return HttpResponse.json(LIST);
      }),
    );
    await expect(
      native.backup.remove("db_backup_20260819_101500.db"),
    ).resolves.toEqual(LIST);
    expect(received).toEqual({ backup: "db_backup_20260819_101500.db" });
  });

  it("sends the stable source filename and the user-facing name when renaming", async () => {
    let received: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_rename`, async ({ request }) => {
        received = await request.json();
        return HttpResponse.json(LIST);
      }),
    );
    await expect(
      native.backup.rename("db_backup_20260819_101500.db", "before-upgrade"),
    ).resolves.toEqual(LIST);
    expect(received).toEqual({
      backup: "db_backup_20260819_101500.db",
      name: "before-upgrade",
    });
  });

  it("refuses a restore response without the sync flag", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_restore`, () =>
        HttpResponse.json({ backups: LIST }),
      ),
    );
    await expect(native.backup.restore("x.db")).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });

  it("exports through the native picker without receiving a path", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_export`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({ status: "exported" });
      }),
    );

    await expect(native.backup.exportArchive()).resolves.toEqual({
      status: "exported",
    });
    expect(body).toEqual({});
  });

  it("imports through the native picker and returns the authoritative list", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_import`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json({
          status: "imported",
          backups: LIST,
          toolsOutOfSync: true,
        });
      }),
    );

    await expect(native.backup.importArchive()).resolves.toEqual({
      status: "imported",
      backups: LIST,
      toolsOutOfSync: true,
    });
    expect(body).toEqual({});
  });

  it("rejects an accidental native-path disclosure in transfer outcomes", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_export`, () =>
        HttpResponse.json({
          status: "exported",
          selectedPath: "/Users/private/config.sql",
        }),
      ),
    );
    await expect(native.backup.exportArchive()).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });

    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_import`, () =>
        HttpResponse.json({
          status: "imported",
          backups: LIST,
          toolsOutOfSync: false,
          sourcePath: "/Users/private/config.sql",
        }),
      ),
    );
    await expect(native.backup.importArchive()).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });

  it("reads the daily on/off policy and retain count", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_schedule_get`, () =>
        HttpResponse.json({ automatic: true, retainCount: 10 }),
      ),
    );
    await expect(native.backupSchedule.get()).resolves.toEqual({
      automatic: true,
      retainCount: 10,
    });
  });

  it("sends the whole policy under `schedule` and keeps the read-back", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_backup_schedule_save`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({ automatic: false, retainCount: 10 });
        },
      ),
    );
    await expect(
      native.backupSchedule.save({ automatic: false, retainCount: 10 }),
    ).resolves.toEqual({ automatic: false, retainCount: 10 });
    expect(body).toEqual({ schedule: { automatic: false, retainCount: 10 } });
  });

  it.each([
    { automatic: true, retainCount: 0 },
    { automatic: true, retainCount: 10, intervalHours: 24 },
  ])("rejects an upstream-shaped schedule %o", async (response) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_backup_schedule_get`, () =>
        HttpResponse.json(response),
      ),
    );
    await expect(native.backupSchedule.get()).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });
});
