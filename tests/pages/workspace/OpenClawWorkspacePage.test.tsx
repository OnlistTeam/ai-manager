import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { OpenClawWorkspacePage } from "@/pages/workspace";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const overview = {
  files: [
    {
      id: "agents",
      filename: "AGENTS.md",
      status: "ready",
      sizeBytes: 14,
      modifiedAt: 1_777_000_000,
    },
    {
      id: "memory",
      filename: "MEMORY.md",
      status: "missing",
      sizeBytes: 0,
      modifiedAt: null,
    },
  ],
  existingFiles: 1,
  dailyMemoryCount: 1,
  dailyMemoryBytes: 18,
  totalBytes: 32,
  limited: false,
};
const memories = {
  items: [
    {
      date: "2026-08-26",
      sizeBytes: 18,
      modifiedAt: 1_777_000_100,
      preview: "Finished the workspace boundary",
      matchCount: 0,
    },
  ],
  totalCount: 1,
  totalBytes: 18,
  limited: false,
};

function mount() {
  return render(<OpenClawWorkspacePage />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("OpenClawWorkspacePage", () => {
  beforeEach(async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_overview`, () =>
        HttpResponse.json(overview),
      ),
      http.post(`${TAURI_ENDPOINT}/app_openclaw_daily_memories`, () =>
        HttpResponse.json(memories),
      ),
      http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_open_directory`, () =>
        HttpResponse.json(null),
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        common: en.common,
        ds: en.ds,
        error: en.error,
        openClawWorkspace: en.openClawWorkspace,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows the fixed files, daily memory and storage without reading file bodies", async () => {
    let documentCalls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_document`, () => {
        documentCalls += 1;
        return HttpResponse.json({
          id: "agents",
          filename: "AGENTS.md",
          exists: true,
          content: "private body",
          sizeBytes: 12,
          modifiedAt: null,
        });
      }),
    );
    mount();

    expect(await screen.findByText("AGENTS.md")).toBeInTheDocument();
    expect(
      await screen.findByText("Finished the workspace boundary"),
    ).toBeInTheDocument();
    expect(screen.getByText("1 of 2")).toBeInTheDocument();
    expect(documentCalls).toBe(0);
    expect(screen.queryByText("private body")).toBeNull();
  });

  it("loads and saves a core file only by its fixed id", async () => {
    const payloads: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_openclaw_workspace_document`,
        async ({ request }) => {
          payloads.push(await request.json());
          return HttpResponse.json({
            id: "agents",
            filename: "AGENTS.md",
            exists: true,
            content: "# Existing rules",
            sizeBytes: 16,
            modifiedAt: null,
          });
        },
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_openclaw_workspace_save_document`,
        async ({ request }) => {
          payloads.push(await request.json());
          return HttpResponse.json({ backupCreated: true });
        },
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: /AGENTS.md/ }),
    );
    const editor = await screen.findByRole("textbox", {
      name: "Workspace Markdown",
    });
    expect(editor).toHaveValue("# Existing rules");
    await userEvent.clear(editor);
    await userEvent.type(editor, "# Safer rules");
    await userEvent.click(screen.getByRole("button", { name: "Save changes" }));

    await waitFor(() =>
      expect(payloads).toEqual([
        { file: "agents" },
        { file: "agents", content: "# Safer rules" },
      ]),
    );
    expect(JSON.stringify(payloads)).not.toMatch(/path|directory/iu);
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "Edit AGENTS.md" }),
      ).toBeNull(),
    );
  });

  it("searches daily memory explicitly instead of rescanning on every keystroke", async () => {
    const payloads: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_openclaw_daily_memories`,
        async ({ request }) => {
          payloads.push(await request.json());
          return HttpResponse.json(memories);
        },
      ),
    );
    mount();
    await screen.findByText("Finished the workspace boundary");
    const search = screen.getByRole("textbox", { name: "Search daily memory" });
    await userEvent.type(search, "workspace");
    expect(payloads).toEqual([{ query: null }]);
    await userEvent.click(screen.getByRole("button", { name: "Search" }));
    await waitFor(() =>
      expect(payloads.at(-1)).toEqual({ query: "workspace" }),
    );
  });

  it("opens a native fixed directory target and never sends a path", async () => {
    let payload: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_openclaw_workspace_open_directory`,
        async ({ request }) => {
          payload = await request.json();
          return HttpResponse.json(null);
        },
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: "Open workspace" }),
    );
    await waitFor(() => expect(payload).toEqual({ directory: "workspace" }));
  });

  it("requires confirmation before deleting a daily memory", async () => {
    let payload: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_openclaw_daily_memory_delete`,
        async ({ request }) => {
          payload = await request.json();
          return HttpResponse.json({ backupCreated: true });
        },
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Delete memory for 2026-08-26",
      }),
    );
    expect(payload).toBeUndefined();
    expect(
      screen.getByRole("dialog", { name: "Delete memory for 2026-08-26?" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Delete memory" }),
    );
    await waitFor(() => expect(payload).toEqual({ date: "2026-08-26" }));
  });
});
