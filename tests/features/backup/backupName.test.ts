import { describe, expect, it } from "vitest";
import type { BackupFile } from "@/entities/backup";
import {
  backupFilenameForName,
  customBackupName,
  normalizeBackupName,
  validateBackupName,
} from "@/features/backup";

const SOURCE: BackupFile = {
  name: "db_backup_20260819_101500.db",
  createdAt: "2026-08-19T10:15:00Z",
  sizeBytes: 1024,
};

describe("backup names", () => {
  it("distinguishes automatic filenames from names chosen by the user", () => {
    expect(customBackupName(SOURCE)).toBeNull();
    expect(customBackupName({ ...SOURCE, name: "before-upgrade.db" })).toBe(
      "before-upgrade",
    );
    expect(normalizeBackupName(" before-upgrade.db ")).toBe("before-upgrade");
    expect(backupFilenameForName("before-upgrade")).toBe("before-upgrade.db");
  });

  it("mirrors the upstream rename safety contract before submission", () => {
    const existing = { ...SOURCE, name: "existing.db" };
    expect(validateBackupName("", SOURCE, [SOURCE])).toBe("required");
    expect(validateBackupName(".db", SOURCE, [SOURCE])).toBe("required");
    expect(validateBackupName("../outside", SOURCE, [SOURCE])).toBe("invalid");
    expect(validateBackupName("folder/name", SOURCE, [SOURCE])).toBe("invalid");
    expect(validateBackupName("x".repeat(101), SOURCE, [SOURCE])).toBe(
      "tooLong",
    );
    expect(validateBackupName("existing", SOURCE, [SOURCE, existing])).toBe(
      "exists",
    );
    expect(
      validateBackupName("db_backup_20260819_101500", SOURCE, [SOURCE]),
    ).toBe("unchanged");
    expect(validateBackupName("before-upgrade", SOURCE, [SOURCE])).toBeNull();
  });

  it("counts UTF-8 bytes exactly like the reused Rust implementation", () => {
    expect(validateBackupName("あ".repeat(33), SOURCE, [SOURCE])).toBeNull();
    expect(validateBackupName("あ".repeat(34), SOURCE, [SOURCE])).toBe(
      "tooLong",
    );
  });
});
