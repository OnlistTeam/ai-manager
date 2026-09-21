import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { delay, http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { skillCatalogKeys } from "@/entities/skill-catalog";
import { SkillCatalogModal } from "@/features/extension-management";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

const CATALOG = [
  {
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
  },
  {
    id: "example-org/skills:release-notes",
    name: "Release notes",
    description: null,
    source: {
      owner: "example-org",
      repository: "skills",
      branch: "main",
      directory: "release-notes",
    },
    installed: true,
    mirrorUsed: false,
  },
];

function mount(onOpenChange = vi.fn(), mutationsBlocked = false) {
  const client = createTestQueryClient();
  return {
    client,
    onOpenChange,
    ...render(
      <SkillCatalogModal
        open
        tool="claude-code"
        toolName="Claude Code"
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onOpenChange}
      />,
      { wrapper: withQueryClient(client) },
    ),
  };
}

describe("SkillCatalogModal", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        extensions: en.extensions,
        taskCenter: en.taskCenter,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.error.mockClear();
    toastMocks.info.mockClear();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, () =>
        HttpResponse.json(CATALOG),
      ),
    );
  });

  it("loads only after opening and focuses the search field", async () => {
    mount();
    const search = await screen.findByLabelText(
      en.extensions.skill.catalog.searchLabel,
    );
    expect(search).toHaveFocus();
    expect(await screen.findByText("Code review")).toBeInTheDocument();
    expect(screen.getByText("From anthropics/skills")).toBeInTheDocument();
    expect(screen.getByText("Code review").closest("ul")).toHaveClass(
      "scrollbar-subtle",
    );
    expect(
      screen.getByText(en.extensions.skill.catalog.trustNote),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("status", {
        name: en.extensions.skill.catalog.mirrorTitle,
      }),
    ).not.toBeInTheDocument();
  });

  it("discloses a trusted transport fallback only after it was used", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, () =>
        HttpResponse.json([{ ...CATALOG[0], mirrorUsed: true }, CATALOG[1]]),
      ),
    );

    mount();
    const notice = await screen.findByRole("status", {
      name: en.extensions.skill.catalog.mirrorTitle,
    });
    expect(notice).toHaveTextContent(
      en.extensions.skill.catalog.mirrorDescription,
    );
    expect(notice).toHaveTextContent("GitHub");
    expect(notice).toHaveTextContent("SHA-256");
  });

  it("keeps long catalog descriptions and repository identities readable", async () => {
    const description =
      "https://docs.example.test/skills/ordinaryuserworkflowmetadatawithoutspacesmustremaincompletelyreadableinsideeveryskillcatalogrow";
    const owner = "ordinaryteamproductexperiencetestgroup";
    const repository =
      "ordinaryuserworkflowscarefullyreviewedautomationskillcatalogrepositoryforlocalfirstdesktopexperiences";
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, () =>
        HttpResponse.json([
          {
            ...CATALOG[0],
            description,
            source: { ...CATALOG[0].source, owner, repository },
          },
        ]),
      ),
    );

    mount();
    const descriptionCopy = await screen.findByText(description);
    const sourceCopy = screen.getByText(`From ${owner}/${repository}`);
    expect(descriptionCopy).toHaveClass(
      "break-words",
      "[overflow-wrap:anywhere]",
    );
    expect(descriptionCopy).not.toHaveClass("truncate");
    expect(sourceCopy).toHaveClass("break-words", "[overflow-wrap:anywhere]");
    expect(sourceCopy).not.toHaveClass("truncate");
    expect(
      screen.getByRole("button", { name: "Install Code review" }),
    ).toBeEnabled();
  });

  it("searches friendly metadata without showing a raw configuration editor", async () => {
    const { container } = mount();
    const search = await screen.findByLabelText(
      en.extensions.skill.catalog.searchLabel,
    );
    await userEvent.type(search, "release");
    expect(screen.queryByText("Code review")).not.toBeInTheDocument();
    expect(screen.getByText("Release notes")).toBeInTheDocument();
    expect(container.querySelector("textarea")).toBeNull();
    expect(container.querySelector("pre")).toBeNull();
  });

  it("manages trusted sources inside the same dialog and restores catalog focus", async () => {
    mount();
    await screen.findByLabelText(en.extensions.skill.catalog.searchLabel);
    await screen.findByText("Code review");

    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.skill.repositories.manage,
      }),
    );
    const sourceDialog = screen.getByRole("dialog", {
      name: en.extensions.skill.repositories.title,
    });
    expect(sourceDialog).toBeInTheDocument();
    expect(
      within(sourceDialog).getByLabelText(
        en.extensions.skill.repositories.location,
      ),
    ).toHaveFocus();

    await userEvent.click(
      within(sourceDialog).getByRole("button", {
        name: en.extensions.skill.repositories.back,
      }),
    );
    expect(
      screen.getByRole("dialog", {
        name: en.extensions.skill.catalog.title,
      }),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByLabelText(en.extensions.skill.catalog.searchLabel),
      ).toHaveFocus(),
    );
  });

  it("explains local ZIP trust and returns to catalog without nesting a dialog", async () => {
    mount();
    await screen.findByText("Code review");

    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.skill.zip.action }),
    );
    const dialog = screen.getByRole("dialog", {
      name: en.extensions.skill.zip.title,
    });
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(dialog).toHaveTextContent(en.extensions.skill.zip.trustTitle);
    expect(dialog).toHaveTextContent(en.extensions.skill.zip.formatDescription);
    expect(dialog).toHaveTextContent(en.extensions.skill.zip.partialNote);
    expect(
      within(dialog).getByRole("button", {
        name: en.extensions.skill.zip.choose,
      }),
    ).toHaveFocus();

    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.extensions.skill.repositories.back,
      }),
    );
    await waitFor(() =>
      expect(
        screen.getByLabelText(en.extensions.skill.catalog.searchLabel),
      ).toHaveFocus(),
    );
  });

  it("opens cached recovery copies in the same dialog and restores catalog focus", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_backups_list`, () =>
        HttpResponse.json([]),
      ),
    );
    mount();
    await screen.findByText("Code review");
    await userEvent.click(
      screen.getByRole("button", {
        name: en.extensions.skill.backups.action,
      }),
    );

    const dialog = screen.getByRole("dialog", {
      name: en.extensions.skill.backups.title,
    });
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(dialog).toHaveTextContent(en.extensions.skill.backups.empty);
    await waitFor(() =>
      expect(
        within(dialog).getByRole("button", {
          name: en.extensions.skill.backups.refresh,
        }),
      ).toHaveFocus(),
    );

    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.extensions.skill.repositories.back,
      }),
    );
    await waitFor(() =>
      expect(
        screen.getByLabelText(en.extensions.skill.catalog.searchLabel),
      ).toHaveFocus(),
    );
  });

  it("starts a native ZIP task with only the tool and closes the catalog", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_skill_zip_install`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            status: "started",
            operation: "op-skill-zip-1",
          });
        },
      ),
    );
    const onOpenChange = vi.fn();
    mount(onOpenChange);
    await screen.findByText("Code review");
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.skill.zip.action }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.skill.zip.choose }),
    );

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({ tool: "claude-code" });
    expect(toastMocks.info).toHaveBeenCalledWith(
      en.extensions.skill.zip.queued,
      { description: en.extensions.skill.zip.queuedDescription },
    );
  });

  it("keeps the ZIP view open and silent when the native picker is cancelled", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_zip_install`, () =>
        HttpResponse.json({ status: "cancelled" }),
      ),
    );
    const onOpenChange = vi.fn();
    mount(onOpenChange);
    await screen.findByText("Code review");
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.skill.zip.action }),
    );
    const choose = screen.getByRole("button", {
      name: en.extensions.skill.zip.choose,
    });
    await userEvent.click(choose);

    await waitFor(() => expect(choose).toBeEnabled());
    expect(
      screen.getByRole("dialog", { name: en.extensions.skill.zip.title }),
    ).toBeInTheDocument();
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(toastMocks.info).not.toHaveBeenCalled();
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("shows a safe ZIP start error without exposing the selected path", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_zip_install`, () =>
        HttpResponse.json(
          {
            code: "INSTALL_FAILED",
            messageKey: "error.skill.zipInvalid",
            technicalMessage: "/Users/private/secret-skills.zip",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    mount();
    await screen.findByText("Code review");
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.skill.zip.action }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.skill.zip.choose }),
    );

    const alert = await screen.findByRole("alert", {
      name: en.error.skill.zipInvalid,
    });
    expect(alert).toHaveTextContent(en.error.skill.zipInvalid);
    expect(alert).toHaveTextContent(en.extensions.skill.zip.errorHint);
    expect(alert).not.toHaveTextContent("/Users/private");
    expect(alert).not.toHaveTextContent("secret-skills.zip");
  });

  it("marks installed rows and never offers to install them twice", async () => {
    mount();
    const row = (await screen.findByText("Release notes")).closest("li");
    expect(row).not.toBeNull();
    expect(
      within(row as HTMLElement).getByRole("button", {
        name: "Release notes is installed",
      }),
    ).toBeDisabled();
    expect(
      within(row as HTMLElement).getAllByText(
        en.extensions.skill.catalog.installed,
      ),
    ).toHaveLength(2);
  });

  it("keeps browsing and Close available while authoritative extension changes are paused", async () => {
    mount(vi.fn(), true);
    const dialog = screen.getByRole("dialog", {
      name: en.extensions.skill.catalog.title,
    });
    const alert = await within(dialog).findByRole("alert", {
      name: en.extensions.actionsPaused.title,
    });
    expect(alert).toHaveTextContent(en.extensions.actionsPaused.description);
    expect(
      within(dialog).getByLabelText(en.extensions.skill.catalog.searchLabel),
    ).toBeEnabled();
    expect(
      await within(dialog).findByRole("button", {
        name: "Install Code review",
      }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", {
        name: en.extensions.skill.zip.action,
      }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", {
        name: en.extensions.skill.backups.action,
      }),
    ).toBeEnabled();
    for (const close of within(dialog).getAllByRole("button", {
      name: en.ds.action.close,
    })) {
      expect(close).toBeEnabled();
    }
  });

  it("queues one install, closes the picker, and points progress to Tasks", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_install`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json("op-skill-1");
      }),
    );
    const onOpenChange = vi.fn();
    mount(onOpenChange);
    await userEvent.click(
      await screen.findByRole("button", { name: "Install Code review" }),
    );

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({ tool: "claude-code", skill: CATALOG[0] });
    expect(toastMocks.info).toHaveBeenCalledWith("Installing Code review", {
      description: "You can keep working and follow progress in Tasks.",
    });
  });

  it("keeps a failed install beside its skill and retries it in place", async () => {
    const bodies: unknown[] = [];
    let attempts = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_install`, async ({ request }) => {
        attempts += 1;
        bodies.push(await request.json());
        if (attempts === 1) {
          return HttpResponse.json(
            {
              code: "INSTALL_FAILED",
              messageKey: "error.skill.installFailed",
              technicalMessage: "/private/catalog token=do-not-show",
              remediation: "error.remediation.checkInternetConnection",
              contextId: null,
            },
            { status: 500 },
          );
        }
        return HttpResponse.json("op-skill-retry");
      }),
    );
    const onOpenChange = vi.fn();
    mount(onOpenChange);
    const row = (await screen.findByText("Code review")).closest("li");
    expect(row).not.toBeNull();

    await userEvent.click(
      within(row as HTMLElement).getByRole("button", {
        name: "Install Code review",
      }),
    );

    const alert = await within(row as HTMLElement).findByRole("alert", {
      name: "Could not start installing Code review",
    });
    expect(alert).toHaveTextContent(en.error.skill.installFailed);
    expect(alert).toHaveTextContent(
      en.error.remediation.checkInternetConnection,
    );
    expect(alert).not.toHaveTextContent("/private/catalog");
    expect(alert).not.toHaveTextContent("do-not-show");
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    expect(toastMocks.error).not.toHaveBeenCalled();

    const retry = within(row as HTMLElement).getByRole("button", {
      name: "Retry installing Code review",
    });
    expect(retry).toHaveFocus();
    await userEvent.click(retry);
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(bodies).toEqual([
      { tool: "claude-code", skill: CATALOG[0] },
      { tool: "claude-code", skill: CATALOG[0] },
    ]);
  });

  it("keeps the first catalog error stable and restores focus across retries", async () => {
    let requests = 0;
    let releaseFailedRetry: (() => void) | undefined;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, async () => {
        requests += 1;
        if (requests === 1) {
          await delay(25);
          return HttpResponse.text("offline", { status: 500 });
        }
        if (requests === 2) {
          await new Promise<void>((resolve) => {
            releaseFailedRetry = resolve;
          });
          return HttpResponse.text("private catalog source", { status: 500 });
        }
        return HttpResponse.json(CATALOG);
      }),
    );
    mount();
    const loading = screen.getByRole("status");
    expect(loading).toHaveTextContent(en.extensions.skill.catalog.loading);
    const indicator = loading.querySelector("svg");
    expect(indicator).toHaveClass("motion-safe:animate-spin");
    expect(indicator?.classList.contains("animate-spin")).toBe(false);
    const alert = await screen.findByRole("alert", {
      name: en.extensions.skill.catalog.errorTitle,
    });
    const retry = within(alert).getByRole("button", {
      name: en.extensions.skill.catalog.retry,
    });
    await userEvent.click(retry);
    await waitFor(() => expect(requests).toBe(2));
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(alert).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByText("private catalog source")).toBeNull();

    act(() => releaseFailedRetry?.());
    await waitFor(() => expect(retry).toBeEnabled());
    expect(retry).toHaveFocus();

    await userEvent.click(retry);
    expect(await screen.findByText("Code review")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.getByLabelText(en.extensions.skill.catalog.searchLabel),
      ).toHaveFocus(),
    );
  });

  it("keeps cached Skills readable but pauses Install until refresh recovers", async () => {
    let requests = 0;
    let installs = 0;
    let releaseRefresh: (() => void) | undefined;
    let releaseRecovery: (() => void) | undefined;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_skill_catalog_list`, async () => {
        requests += 1;
        if (requests === 1) return HttpResponse.json(CATALOG);
        if (requests === 2) {
          await new Promise<void>((resolve) => {
            releaseRefresh = resolve;
          });
          return HttpResponse.text("private catalog source", { status: 500 });
        }
        await new Promise<void>((resolve) => {
          releaseRecovery = resolve;
        });
        return HttpResponse.json(CATALOG);
      }),
      http.post(`${TAURI_ENDPOINT}/app_skill_install`, () => {
        installs += 1;
        return HttpResponse.json("op-skill-stale");
      }),
    );
    const { client } = mount();
    const search = await screen.findByLabelText(
      en.extensions.skill.catalog.searchLabel,
    );
    const install = await screen.findByRole("button", {
      name: "Install Code review",
    });

    act(() => {
      void client.invalidateQueries({ queryKey: skillCatalogKeys.all });
    });
    await waitFor(() => expect(requests).toBe(2));
    expect(
      screen.getByRole("status", {
        name: "Checking the latest Skill catalog",
      }),
    ).toBeInTheDocument();
    expect(screen.getByText("Code review")).toBeInTheDocument();
    expect(install).toBeDisabled();
    await userEvent.click(install);
    expect(installs).toBe(0);

    act(() => releaseRefresh?.());
    const warning = await screen.findByRole("alert", {
      name: "Couldn't refresh the Skill catalog",
    });
    expect(warning).toHaveTextContent(
      en.extensions.skill.catalog.refreshErrorDescription,
    );
    expect(screen.getByText("Code review")).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("private catalog source");
    expect(install).toBeDisabled();

    const retry = within(warning).getByRole("button", {
      name: en.extensions.skill.catalog.retry,
    });
    await userEvent.click(retry);
    await waitFor(() => expect(requests).toBe(3));
    expect(within(warning).getByRole("button")).toBe(retry);
    expect(retry).toHaveAttribute("aria-busy", "true");
    expect(retry).toBeDisabled();
    expect(warning).toHaveAttribute("aria-busy", "true");
    expect(install).toBeDisabled();

    act(() => releaseRecovery?.());
    await waitFor(() => expect(warning).not.toBeInTheDocument());
    expect(search).toHaveFocus();
    expect(install).toBeEnabled();
  });
});
