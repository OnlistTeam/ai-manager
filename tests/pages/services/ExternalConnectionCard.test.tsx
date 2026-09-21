import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { ExternalConnectionCard } from "@/pages/services/ExternalConnectionCard";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

it("copies the visible external URL without changing selection", async () => {
  i18n.addResourceBundle("en", "translation", en, true, true);
  await i18n.changeLanguage("en");
  const user = userEvent.setup();
  const writeText = vi.spyOn(navigator.clipboard, "writeText");
  const endpoint = "https://external.example/v1";
  render(
    <ExternalConnectionCard
      tool="opencode"
      toolName="OpenCode"
      connection={{
        selection: "recentModel",
        model: "relay/model",
        endpoint,
        endpointSource: { kind: "liveConfig", path: "config.json" },
        credential: "unknown",
        credentialSource: { kind: "toolDefault" },
        providerId: null,
        shellInspected: true,
      }}
    />,
    { wrapper: withQueryClient(createTestQueryClient()) },
  );
  expect(screen.getByText(endpoint)).toBeVisible();
  await user.click(
    screen.getByRole("button", {
      name: en.ds.action.copyNamed.replace(
        "{{name}}",
        en.services.external.endpoint,
      ),
    }),
  );
  expect(writeText).toHaveBeenCalledWith(endpoint);
  expect(screen.getByText(en.services.card.recentModel)).toBeVisible();
  expect(screen.queryByText(en.services.card.inUse)).toBeNull();
});
