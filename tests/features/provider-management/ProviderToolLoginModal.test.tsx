import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import type { ComponentProps } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { ProviderToolLoginModal } from "@/features/provider-management";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

type ModalProps = ComponentProps<typeof ProviderToolLoginModal>;

function renderModal(overrides: Partial<ModalProps> = {}) {
  const props: ModalProps = {
    account: "chatGpt",
    tool: "codex",
    toolName: "Codex CLI",
    saved: true,
    onOpenChange: vi.fn(),
    onRestore: vi.fn(),
    onSignedIn: vi.fn(),
    ...overrides,
  };
  render(<ProviderToolLoginModal {...props} />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
  return props;
}

function progress(overrides: Record<string, unknown> = {}) {
  return {
    id: "flow-1",
    phase: "waiting",
    url: "https://auth.openai.com/oauth/authorize?state=s",
    code: null,
    account: null,
    failure: null,
    created: null,
    ...overrides,
  };
}

const login = en.services.login;

describe("ProviderToolLoginModal", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, services: en.services },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("signs in from AI Manager, waits for the browser, then hands over the endpoint", async () => {
    const started: unknown[] = [];
    let polls = 0;
    const created = {
      providers: [
        {
          id: "codex-account-1",
          tool: "codex",
          name: "ChatGPT · someone@example.com",
          kind: "official",
          active: false,
          baseUrl: null,
          apiKey: null,
          websiteUrl: null,
          testable: false,
          canRemove: true,
          accountBound: true,
        },
      ],
      createdProviderId: "codex-account-1",
    };
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_sign_in_start`,
        async ({ request }) => {
          started.push(await request.json());
          return HttpResponse.json(progress());
        },
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_sign_in_status`, () => {
        polls += 1;
        return HttpResponse.json(
          polls < 2
            ? progress()
            : progress({
                phase: "done",
                account: "someone@example.com",
                created,
              }),
        );
      }),
    );
    const props = renderModal({
      status: { state: "signedOut", account: null, plan: null },
    });
    expect(screen.getByRole("status")).toHaveTextContent(
      `Codex CLI right now: ${login.state.signedOut}`,
    );
    await userEvent.click(screen.getByRole("button", { name: login.signIn }));
    expect(started).toEqual([{ tool: "codex" }]);
    expect(await screen.findByText(login.waiting)).toBeVisible();
    expect(screen.getByRole("button", { name: login.openAgain })).toBeVisible();
    // Waiting, there is nothing to press but Cancel.
    expect(screen.queryByRole("button", { name: login.signIn })).toBeNull();
    await waitFor(
      () => expect(props.onSignedIn).toHaveBeenCalledWith(created),
      {
        timeout: 4000,
      },
    );
  });

  it("shows the code a device sign-in asks for", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_sign_in_start`, () =>
        HttpResponse.json(
          progress({ url: "https://accounts.x.ai/device", code: "ABCD-EFGH" }),
        ),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_sign_in_status`, () =>
        HttpResponse.json(
          progress({ url: "https://accounts.x.ai/device", code: "ABCD-EFGH" }),
        ),
      ),
    );
    renderModal({
      account: "superGrok",
      tool: "grok-build",
      toolName: "Grok Build",
    });
    await userEvent.click(screen.getByRole("button", { name: login.signIn }));
    expect(await screen.findByText("ABCD-EFGH")).toBeVisible();
    expect(screen.getByText(login.code)).toBeVisible();
  });

  it("says why a sign-in failed and offers to try again", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_sign_in_start`, () =>
        HttpResponse.json(
          progress({ phase: "failed", url: null, failure: "portBusy" }),
        ),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_sign_in_status`, () =>
        HttpResponse.json(
          progress({ phase: "failed", url: null, failure: "portBusy" }),
        ),
      ),
    );
    renderModal();
    await userEvent.click(screen.getByRole("button", { name: login.signIn }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      login.failure.portBusy,
    );
    expect(screen.getByRole("button", { name: login.tryAgain })).toBeEnabled();
  });

  it("cancels a sign-in still waiting when the dialog closes", async () => {
    const canceled: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_sign_in_start`, () =>
        HttpResponse.json(progress()),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_sign_in_status`, () =>
        HttpResponse.json(progress()),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_sign_in_cancel`,
        async ({ request }) => {
          canceled.push(await request.json());
          return HttpResponse.json(null);
        },
      ),
    );
    const props = renderModal();
    await userEvent.click(screen.getByRole("button", { name: login.signIn }));
    await screen.findByText(login.waiting);
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    );
    await waitFor(() =>
      expect(canceled).toEqual([{ tool: "codex", id: "flow-1" }]),
    );
    expect(props.onOpenChange).toHaveBeenCalledWith(false);
  });

  it("offers another account beside a signed-in one, and names it", () => {
    renderModal({
      account: "claude",
      tool: "claude-code",
      toolName: "Claude Code",
      status: {
        state: "signedIn",
        account: "someone@example.com",
        plan: "pro",
      },
    });
    expect(screen.getByRole("status")).toHaveTextContent(
      `${login.state.signedIn} · someone@example.com · Pro`,
    );
    expect(
      screen.getByRole("button", { name: login.signInAnother }),
    ).toBeEnabled();
    expect(screen.getByText(login.browserHintSeveral)).toBeVisible();
  });

  it("claims nothing while the tool could not say", () => {
    renderModal({ status: { state: "unknown", account: null, plan: null } });
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("puts back the entry that follows the tool's own sign-in only when it is gone", async () => {
    const props = renderModal({ saved: false });
    const dialog = screen.getByRole("dialog");
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: login.useToolLogin.replace("{{tool}}", "Codex CLI"),
      }),
    );
    expect(props.onRestore).toHaveBeenCalled();
  });
});
