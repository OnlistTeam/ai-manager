import { describe, expect, it } from "vitest";
import { backupLabel, backupSizeMb } from "@/features/backup";
import type { BackupFile } from "@/entities/backup";

function file(overrides: Partial<BackupFile> = {}): BackupFile {
  return {
    name: "db_backup_20260819_101500.db",
    createdAt: "2026-08-19T10:15:00Z",
    sizeBytes: 2_202_009,
    ...overrides,
  };
}

describe("backupLabel", () => {
  it("says when the backup was made, not what the file is called", () => {
    // Spec §99: users shouldn't need to recognize db_backup_20260819_101500.db.
    const label = backupLabel(file(), "en-US");
    expect(label).not.toContain("db_backup");
    expect(label).toContain("2026");
  });

  it("follows the interface language", () => {
    expect(backupLabel(file(), "en-US")).not.toEqual(
      backupLabel(file(), "ja-JP"),
    );
  });

  it("shows a user-chosen name instead of replacing it with the timestamp", () => {
    expect(
      backupLabel(file({ name: "before-upgrading-claude.db" }), "en-US"),
    ).toBe("before-upgrading-claude");
  });

  it("falls back to the file name when the timestamp is unusable", () => {
    // The backend uses the file's timestamp; it's an empty string when unavailable (database/backup.rs:995).
    for (const createdAt of ["", "not a date"]) {
      expect(backupLabel(file({ createdAt }), "en-US")).toBe(
        "db_backup_20260819_101500.db",
      );
    }
  });
});

describe("backupSizeMb", () => {
  it("rounds to one decimal", () => {
    expect(backupSizeMb(file({ sizeBytes: 2_202_009 }))).toBe("2.1");
  });

  it("never shows a scary 0.0", () => {
    expect(backupSizeMb(file({ sizeBytes: 1_024 }))).toBe("0.1");
  });
});
