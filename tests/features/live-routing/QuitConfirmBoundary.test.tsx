import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import { QuitConfirmBoundary } from "@/features/live-routing";
import en from "@/i18n/locales/en.json";
import { QUIT_REQUESTED_EVENT } from "@/native";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { emitTauriEvent } from "../../msw/tauriMocks";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

async function askToQuit(tools: unknown[]) {
  await waitFor(() => {
    act(() => emitTauriEvent(QUIT_REQUESTED_EVENT, { tools }));
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
}

describe("QuitConfirmBoundary", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, routing: en.routing },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("names the routed tools and which open sessions need a restart", async () => {
    let confirmed = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_quit_confirmed`, () => {
        confirmed += 1;
        return HttpResponse.json(null);
      }),
    );
    render(<QuitConfirmBoundary />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    expect(screen.queryByRole("dialog")).toBeNull();

    await askToQuit([
      { tool: "claude-code", pickup: "live" },
      { tool: "codex", pickup: "atStart" },
    ]);
    const dialog = screen.getByRole("dialog", { name: en.routing.quit.title });
    expect(dialog).toHaveTextContent(
      "Routed through AI Manager: Claude Code, Codex.",
    );
    expect(dialog).toHaveTextContent(
      "Open sessions of Claude Code switch back on their own",
    );
    expect(dialog).toHaveTextContent(
      "Restart open sessions of Codex after quitting",
    );

    await userEvent.click(
      screen.getByRole("button", { name: en.routing.quit.confirm }),
    );
    await waitFor(() => expect(confirmed).toBe(1));
  });

  it("keeps AI Manager running when the user cancels", async () => {
    render(<QuitConfirmBoundary />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await askToQuit([{ tool: "codex", pickup: "atStart" }]);
    expect(document.querySelector('[data-quit-pickup="live"]')).toBeNull();

    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
