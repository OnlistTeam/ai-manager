import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { DeepLinkPreview } from "@/entities/deeplink";
import { DeepLinkImportBoundary } from "@/features/deep-link-import";
import en from "@/i18n/locales/en.json";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

function preview(overrides: Partial<DeepLinkPreview> = {}): DeepLinkPreview {
  return {
    id: "pending-1",
    origin: "argv",
    resource: "provider",
    name: "Example endpoint",
    endpoint: "https://api.example.test/v1",
    items: [],
    targets: [{ tool: "claude-code", name: "Claude Code", supported: true }],
    credentialFields: [],
    blocked: null,
    expiresAt: 1_800_000_000,
    ...overrides,
  };
}

function mountWith(previews: DeepLinkPreview[]) {
  let remaining = previews;
  const confirm = vi.fn();
  const dismiss = vi.fn();

  server.use(
    http.post(`${TAURI_ENDPOINT}/app_deeplink_pending_list`, () =>
      HttpResponse.json(remaining),
    ),
    http.post(`${TAURI_ENDPOINT}/app_deeplink_confirm`, async ({ request }) => {
      const body = (await request.json()) as { pending: string };
      confirm(body.pending);
      remaining = remaining.filter((entry) => entry.id !== body.pending);
      return HttpResponse.json({
        resource: "provider",
        tools: ["claude-code"],
        operations: [],
        applied: 1,
      });
    }),
    http.post(`${TAURI_ENDPOINT}/app_deeplink_dismiss`, async ({ request }) => {
      const body = (await request.json()) as { pending: string };
      dismiss(body.pending);
      remaining = remaining.filter((entry) => entry.id !== body.pending);
      return HttpResponse.json(null);
    }),
  );

  const client = createTestQueryClient();
  const view = render(<DeepLinkImportBoundary />, {
    wrapper: withQueryClient(client),
  });
  return { ...view, confirm, dismiss, client };
}

describe("DeepLinkImportBoundary", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, deeplink: en.deeplink },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows what a waiting link will change and where it came from", async () => {
    mountWith([preview()]);

    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByRole("heading", { name: en.deeplink.confirm.title }),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(en.deeplink.origin.argv)).toBeVisible();
    expect(within(dialog).getByText("Example endpoint")).toBeVisible();
    expect(
      within(dialog).getByText("https://api.example.test/v1"),
    ).toBeVisible();
    expect(within(dialog).getByText("Claude Code")).toBeVisible();
  });

  it("imports nothing until the user accepts, and consumes the link when they do", async () => {
    const { confirm, dismiss } = mountWith([preview()]);
    await screen.findByRole("dialog");

    expect(confirm).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: en.deeplink.confirm.action }),
    );

    await waitFor(() => expect(confirm).toHaveBeenCalledWith("pending-1"));
    expect(dismiss).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("discards the link on cancel without importing anything", async () => {
    const { confirm, dismiss } = mountWith([preview()]);
    await screen.findByRole("dialog");

    await userEvent.click(
      screen.getByRole("button", { name: en.deeplink.confirm.cancel }),
    );

    await waitFor(() => expect(dismiss).toHaveBeenCalledWith("pending-1"));
    expect(confirm).not.toHaveBeenCalled();
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("answers several waiting links one at a time", async () => {
    const { confirm } = mountWith([
      preview(),
      preview({ id: "pending-2", name: "Second endpoint" }),
    ]);
    await screen.findByRole("dialog");
    expect(screen.getByText("Example endpoint")).toBeVisible();

    await userEvent.click(
      screen.getByRole("button", { name: en.deeplink.confirm.action }),
    );

    expect(await screen.findByText("Second endpoint")).toBeVisible();
    await waitFor(() => expect(confirm).toHaveBeenCalledTimes(1));
  });

  it("names the field holding a credential and never a credential value", async () => {
    mountWith([preview({ origin: "paste", credentialFields: ["apiKey"] })]);

    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText(en.deeplink.credential.apiKey),
    ).toBeVisible();
    expect(within(dialog).getByText(en.deeplink.origin.paste)).toBeVisible();
    expect(dialog.textContent ?? "").not.toMatch(/sk-/);
  });

  it("explains a blocked link and refuses to offer its action", async () => {
    mountWith([
      preview({
        blocked: "credentialRequired",
        targets: [
          { tool: "claude-code", name: "Claude Code", supported: true },
        ],
      }),
    ]);

    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText(en.deeplink.blocked.credentialRequired),
    ).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: en.deeplink.confirm.action }),
    ).toBeDisabled();
  });

  it("describes an unmanaged target instead of echoing the link's own wording", async () => {
    mountWith([
      preview({
        blocked: "noSupportedTool",
        targets: [{ tool: null, name: null, supported: false }],
      }),
    ]);

    const dialog = await screen.findByRole("dialog");
    expect(
      within(dialog).getByText(en.deeplink.summary.unmanagedTarget),
    ).toBeVisible();
    expect(
      within(dialog).getByText(en.deeplink.blocked.noSupportedTool),
    ).toBeVisible();
  });

  it("keeps the link on screen and recoverable when the import fails", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_deeplink_pending_list`, () =>
        HttpResponse.json([preview()]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_deeplink_confirm`, () =>
        HttpResponse.json(
          {
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.deepLink.credentialRequired",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          },
          { status: 400 },
        ),
      ),
    );
    const client = createTestQueryClient();
    render(<DeepLinkImportBoundary />, { wrapper: withQueryClient(client) });

    await userEvent.click(
      await screen.findByRole("button", { name: en.deeplink.confirm.action }),
    );

    const dialog = await screen.findByRole("dialog");
    expect(
      await within(dialog).findByText(en.error.deepLink.credentialRequired),
    ).toBeVisible();
    expect(within(dialog).getByText(en.deeplink.error.retry)).toBeVisible();
    expect(
      within(dialog).getByRole("button", { name: en.deeplink.confirm.retry }),
    ).toBeEnabled();
  });
});
