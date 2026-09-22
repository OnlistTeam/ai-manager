import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { ExtensionLocationRow } from "@/pages/extensions/ExtensionLocationRow";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const SCOPE = { kind: "tool", id: "codex" } as const;

function describeLocation(
  path = "~/.codex/config.toml",
  exists = true,
  kind = "mcp",
) {
  return http.post("http://tauri.local/app_extension_location_describe", () =>
    HttpResponse.json({ kind, path, exists }),
  );
}

function mount(kind: "mcp" | "prompt" = "mcp") {
  return render(<ExtensionLocationRow scope={SCOPE} kind={kind} />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("ExtensionLocationRow", () => {
  beforeEach(async () => {
    i18n.addResourceBundle("en", "translation", en, true, true);
    await i18n.changeLanguage("en");
  });

  /**
   * The whole point of the row: someone can see which file the product is
   * about to change, without opening anything.
   */
  it("names the file the list is written into", async () => {
    server.use(describeLocation());
    mount();
    expect(await screen.findByText("~/.codex/config.toml")).toBeVisible();
  });

  it.each(["edit", "browse"] as const)(
    "opens the file with action %s, sending the scope rather than a path",
    async (action) => {
      const payloads: unknown[] = [];
      server.use(
        describeLocation(),
        http.post(
          "http://tauri.local/app_extension_location_open",
          async ({ request }) => {
            payloads.push(await request.json());
            return HttpResponse.json(null);
          },
        ),
      );
      mount();
      await userEvent.click(
        await screen.findByRole("button", {
          name:
            action === "edit"
              ? en.extensions.location.open
              : en.extensions.location.action,
        }),
      );
      await waitFor(() =>
        expect(payloads).toEqual([{ scope: SCOPE, kind: "mcp", action }]),
      );
    },
  );

  /**
   * A tool that has never written its config is the normal first-run state.
   * Opening a file that is not there would hand the editor a dead path, so
   * the action is withheld and the state is stated instead.
   */
  it("says so, rather than offering to open nothing, before the file exists", async () => {
    server.use(describeLocation("~/.codex/config.toml", false));
    mount();
    expect(
      await screen.findByText(en.extensions.location.notYetWritten),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: en.extensions.location.open }),
    ).toBeDisabled();
    // The folder can still be shown: the backend falls back to the nearest
    // existing one, which is how someone finds out where it would go.
    expect(
      screen.getByRole("button", { name: en.extensions.location.action }),
    ).toBeEnabled();
  });

  /** Skills keep one directory per entry, so there is no shared file to name. */
  it("renders nothing when the scope keeps no shared file", async () => {
    server.use(
      http.post("http://tauri.local/app_extension_location_describe", () =>
        HttpResponse.json(null),
      ),
    );
    const { container } = mount();
    await waitFor(() => expect(container).toBeEmptyDOMElement());
  });

  it("reports an opener failure and lets the same button retry", async () => {
    let calls = 0;
    server.use(
      describeLocation(),
      http.post("http://tauri.local/app_extension_location_open", () =>
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
    mount();
    const button = await screen.findByRole("button", {
      name: en.extensions.location.action,
    });

    await userEvent.click(button);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.error.extension.resourceOpenFailed,
    );

    await userEvent.click(button);
    await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
    expect(calls).toBe(2);
  });
});
