import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import type { EffectiveConnection } from "@/entities/provider";
import { ExternalConnectionCard } from "@/pages/services/ExternalConnectionCard";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const CONNECTION: EffectiveConnection = {
  selection: "recentModel",
  model: "relay/model",
  endpoint: "https://external.example/v1",
  endpointSource: { kind: "liveConfig", path: "config.json" },
  credential: "unknown",
  credentialSource: { kind: "toolDefault" },
  providerId: null,
  shellInspected: true,
};

function mount(
  overrides: Partial<EffectiveConnection> = {},
  onTest?: () => void,
) {
  return render(
    <ExternalConnectionCard
      tool="opencode"
      toolName="OpenCode"
      connection={{ ...CONNECTION, ...overrides }}
      onTest={onTest}
    />,
    { wrapper: withQueryClient(createTestQueryClient()) },
  );
}

beforeEach(async () => {
  i18n.addResourceBundle("en", "translation", en, true, true);
  await i18n.changeLanguage("en");
});

it("copies the visible external URL without changing selection", async () => {
  const user = userEvent.setup();
  const writeText = vi.spyOn(navigator.clipboard, "writeText");
  mount();
  expect(screen.getByText(CONNECTION.endpoint as string)).toBeVisible();
  await user.click(
    screen.getByRole("button", {
      name: en.ds.action.copyNamed.replace(
        "{{name}}",
        en.services.external.endpoint,
      ),
    }),
  );
  expect(writeText).toHaveBeenCalledWith(CONNECTION.endpoint);
  expect(screen.getByText(en.services.card.recentModel)).toBeVisible();
  expect(screen.queryByText(en.services.card.inUse)).toBeNull();
});

/**
 * The card used to say only "not saved here", which told the reader where a
 * record lived rather than what the setting does. What matters is that this
 * address beats every choice in the list below it.
 */
it("says this address wins over anything chosen in the list", () => {
  mount();
  expect(
    screen.getByText(
      en.services.external.overrides.replace("{{name}}", "OpenCode"),
    ),
  ).toBeVisible();
});

const TEST_LABEL = en.services.action.testNamed.replace(
  "{{name}}",
  "external.example",
);

describe("the test action", () => {
  it("is offered for a connection whose credential the backend can read", async () => {
    const onTest = vi.fn();
    mount({ credential: "configured" }, onTest);

    await userEvent.click(screen.getByRole("button", { name: TEST_LABEL }));
    expect(onTest).toHaveBeenCalledTimes(1);
  });

  it("is offered for a custom address with no key at all", () => {
    mount({ credential: "missing" }, vi.fn());
    expect(screen.getByRole("button", { name: TEST_LABEL })).toBeVisible();
  });

  /**
   * A tool's own OAuth login leaves nothing to send as a bearer token, so a
   * test could only ever fail. No button beats a button that always fails.
   */
  it("is withheld when there is no credential to replay", () => {
    for (const credential of ["toolLogin", "unknown"] as const) {
      const view = mount({ credential }, vi.fn());
      expect(screen.queryByRole("button", { name: TEST_LABEL })).toBeNull();
      view.unmount();
    }
  });

  it("is withheld when the page offers no test handler", () => {
    mount({ credential: "configured" });
    expect(screen.queryByRole("button", { name: TEST_LABEL })).toBeNull();
  });
});
