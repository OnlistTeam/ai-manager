import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { SessionRevealFolderButton } from "@/pages/sessions/SessionRevealFolderButton";
import { TooltipProvider } from "@/shared/ui/Tooltip";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
/** Session references are opaque 64-character hashes at the native boundary. */
const reference = "c".repeat(64);

const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  success: vi.fn(),
  info: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

describe("SessionRevealFolderButton", () => {
  beforeEach(async () => {
    toastMocks.error.mockClear();
    i18n.addResourceBundle(
      "en",
      "translation",
      { error: en.error, sessions: en.sessions },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("still reports a failure that arrives after the button has gone away", async () => {
    // The button lives in the thread panel, which unmounts whenever the
    // selection changes; a per-call callback would have been dropped here.
    let fail: (() => void) | undefined;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_reveal`, async () => {
        await new Promise<void>((resolve) => {
          fail = resolve;
        });
        return HttpResponse.json(
          {
            code: "LAUNCH_FAILED",
            messageKey: "error.system.revealFailed",
            technicalMessage: null,
            remediation: "error.remediation.checkPermissions",
            contextId: null,
          },
          { status: 500 },
        );
      }),
    );
    const Wrapper = withQueryClient(createTestQueryClient());
    const { unmount } = render(
      <Wrapper>
        <TooltipProvider>
          <SessionRevealFolderButton reference={reference} title="Release" />
        </TooltipProvider>
      </Wrapper>,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Show Release in the file manager" }),
    );
    await waitFor(() => expect(fail).toBeDefined());
    unmount();
    fail?.();

    await waitFor(() =>
      expect(toastMocks.error).toHaveBeenCalledWith(
        en.error.system.revealFailed,
        { description: en.error.remediation.checkPermissions },
      ),
    );
  });
});
