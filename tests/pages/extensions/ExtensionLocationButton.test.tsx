import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { ExtensionLocationButton } from "@/pages/extensions/ExtensionLocationButton";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

describe("ExtensionLocationButton", () => {
  beforeEach(async () => {
    i18n.addResourceBundle("en", "translation", en, true, true);
    await i18n.changeLanguage("en");
  });
  it.each(["mcp", "prompt"] as const)(
    "reveals %s by scope only, without passing a renderer path",
    async (kind) => {
      const payloads: unknown[] = [];
      server.use(
        http.post(
          "http://tauri.local/app_extension_location_reveal",
          async ({ request }) => {
            payloads.push(await request.json());
            return HttpResponse.json(null);
          },
        ),
      );
      render(
        <ExtensionLocationButton
          scope={{ kind: "tool", id: "codex" }}
          kind={kind}
        />,
        { wrapper: withQueryClient(createTestQueryClient()) },
      );
      await userEvent.click(
        screen.getByRole("button", { name: en.extensions.location.action }),
      );
      await waitFor(() =>
        expect(payloads).toEqual([
          { scope: { kind: "tool", id: "codex" }, kind },
        ]),
      );
    },
  );
  it("reports an opener error and lets the same button retry", async () => {
    let calls = 0;
    server.use(
      http.post("http://tauri.local/app_extension_location_reveal", () =>
        ++calls === 1
          ? HttpResponse.json(
              {
                code: "LAUNCH_FAILED",
                messageKey: "error.extension.resourceOpenFailed",
                technicalMessage: null,
                remediation: null,
                contextId: null,
              },
              { status: 500 },
            )
          : HttpResponse.json(null),
      ),
    );
    render(
      <ExtensionLocationButton
        scope={{ kind: "desktopApp", id: "claude-desktop" }}
        kind="mcp"
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.location.action }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.error.extension.resourceOpenFailed,
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.location.action }),
    );
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    expect(calls).toBe(2);
  });
});
