import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { DeepLinkPasteSection } from "@/features/deep-link-import";
import en from "@/i18n/locales/en.json";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

function mount() {
  const submitted = vi.fn();
  server.use(
    http.post(
      `${TAURI_ENDPOINT}/app_deeplink_submit_pasted`,
      async ({ request }) => {
        const body = (await request.json()) as { link: string };
        submitted(body.link);
        return HttpResponse.json({
          id: "pending-1",
          origin: "paste",
          resource: "provider",
          name: "Example endpoint",
          endpoint: "https://api.example.test/v1",
          items: [],
          targets: [
            { tool: "claude-code", name: "Claude Code", supported: true },
          ],
          credentialFields: ["apiKey"],
          blocked: null,
          expiresAt: 1_800_000_000,
        });
      },
    ),
  );

  const client = createTestQueryClient();
  const view = render(<DeepLinkPasteSection />, {
    wrapper: withQueryClient(client),
  });
  return { ...view, submitted };
}

describe("DeepLinkPasteSection", () => {
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

  it("hands a pasted link to native and clears the field once it is queued", async () => {
    const { submitted } = mount();
    const field = screen.getByLabelText(en.deeplink.paste.label);

    await userEvent.type(
      field,
      "aimanager://v1/import?resource=provider&app=claude&name=Example",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.deeplink.paste.action }),
    );

    await waitFor(() =>
      expect(submitted).toHaveBeenCalledWith(
        "aimanager://v1/import?resource=provider&app=claude&name=Example",
      ),
    );
    await waitFor(() => expect(field).toHaveValue(""));
  });

  it("cannot submit an empty field", async () => {
    mount();
    expect(
      screen.getByRole("button", { name: en.deeplink.paste.action }),
    ).toBeDisabled();
  });

  /**
   * The upstream scheme is accepted here on purpose (ADR-0029 decision 4), but
   * the interface never mentions the upstream product: the renderer just sends
   * whatever was pasted and lets native decide.
   */
  it("passes an unfamiliar scheme through without naming it in the interface", async () => {
    const { submitted, container } = mount();

    await userEvent.type(
      screen.getByLabelText(en.deeplink.paste.label),
      "ccswitch://v1/import?resource=provider&app=claude&name=Example",
    );
    await userEvent.keyboard("{Enter}");

    await waitFor(() =>
      expect(submitted).toHaveBeenCalledWith(
        "ccswitch://v1/import?resource=provider&app=claude&name=Example",
      ),
    );
    expect(container.textContent ?? "").not.toMatch(/cc\s*switch/i);
  });

  it("keeps the pasted text and explains the refusal when native rejects it", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_deeplink_submit_pasted`, () =>
        HttpResponse.json(
          {
            code: "CONFIG_PARSE_FAILED",
            messageKey: "error.deepLink.invalidLink",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          },
          { status: 400 },
        ),
      ),
    );
    const client = createTestQueryClient();
    render(<DeepLinkPasteSection />, { wrapper: withQueryClient(client) });
    const field = screen.getByLabelText(en.deeplink.paste.label);

    await userEvent.type(field, "not-a-link");
    await userEvent.click(
      screen.getByRole("button", { name: en.deeplink.paste.action }),
    );

    expect(
      await screen.findByText(en.error.deepLink.invalidLink),
    ).toBeVisible();
    expect(field).toHaveValue("not-a-link");
    expect(field).toHaveAttribute("aria-invalid", "true");
  });
});
