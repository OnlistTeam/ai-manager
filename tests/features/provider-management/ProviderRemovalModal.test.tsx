import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Provider } from "@/entities/provider";
import { ProviderRemovalModal } from "@/features/provider-management";
import en from "@/i18n/locales/en.json";
import { NativeError } from "@/native";

const PROVIDER: Provider = {
  id: "relay",
  tool: "claude-code",
  name: "My Relay",
  kind: "custom",
  active: false,
  baseUrl: "https://relay.example.test",
  apiKey: "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F",
  websiteUrl: null,
  testable: true,
  canRemove: true,
};

interface MountOptions {
  provider?: Provider | null;
  busy?: boolean;
  error?: Error | null;
  mutationsBlocked?: boolean;
  onOpenChange?: (open: boolean) => void;
  onConfirm?: () => void;
}

function mount({
  provider = PROVIDER,
  busy = false,
  error = null,
  mutationsBlocked = false,
  onOpenChange = vi.fn(),
  onConfirm = vi.fn(),
}: MountOptions = {}) {
  return {
    onOpenChange,
    onConfirm,
    ...render(
      <ProviderRemovalModal
        provider={provider}
        toolName="Claude Code"
        busy={busy}
        error={error}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onOpenChange}
        onConfirm={onConfirm}
      />,
    ),
  };
}

describe("ProviderRemovalModal", () => {
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

  it("defaults to Cancel and explains every removal boundary", () => {
    mount();
    const dialog = screen.getByRole("dialog", {
      name: en.services.remove.title.replace("{{name}}", PROVIDER.name),
    });

    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toHaveFocus();
    expect(dialog).toHaveTextContent(
      en.services.remove.point.local.replace("{{tool}}", "Claude Code"),
    );
    expect(dialog).toHaveTextContent(en.services.remove.point.account);
    expect(dialog).toHaveTextContent(en.services.remove.point.otherTools);
  });

  it("shows the complete custom service name before destructive confirmation", () => {
    const provider = {
      ...PROVIDER,
      name: "SharedTeamProductionProviderConnectionUsedByEveryDesignerAndDeveloperInThisWholeOrganization",
    };
    mount({ provider });
    const title = en.services.remove.title.replace("{{name}}", provider.name);
    const dialog = screen.getByRole("dialog", { name: title });
    const heading = within(dialog).getByRole("heading", { name: title });

    expect(heading).toHaveClass("break-words");
    expect(heading).not.toHaveAttribute("title");
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toHaveFocus();
  });

  it("asks its caller to remove only after explicit confirmation", async () => {
    const { onConfirm } = mount();

    await userEvent.click(
      screen.getByRole("button", { name: en.services.remove.confirm }),
    );

    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("renders a safe, named failure and turns the same action into Retry", () => {
    const error = new NativeError({
      code: "CONFIG_WRITE_FAILED",
      messageKey: "error.provider.removeRestoreFailed",
      technicalMessage: "/private/config token=do-not-show",
      remediation: "error.remediation.retryOrViewDetails",
      contextId: null,
    });
    const { rerender, onConfirm, onOpenChange } = mount();
    const action = screen.getByRole("button", {
      name: en.services.remove.confirm,
    });
    action.focus();
    rerender(
      <ProviderRemovalModal
        provider={PROVIDER}
        toolName="Claude Code"
        busy={false}
        error={error}
        onOpenChange={onOpenChange}
        onConfirm={onConfirm}
      />,
    );

    const alert = screen.getByRole("alert", {
      name: en.services.remove.errorTitle,
    });
    expect(alert).toHaveTextContent(en.error.provider.removeRestoreFailed);
    expect(alert).toHaveTextContent(en.error.remediation.retryOrViewDetails);
    expect(alert).toHaveTextContent(en.services.remove.errorRetry);
    // The technical detail is never the primary copy: it waits, folded, behind
    // the View details the remediation just pointed at.
    const technical = within(alert).getByText(
      "/private/config token=do-not-show",
    );
    expect(technical).not.toBeVisible();
    const retry = screen.getByRole("button", {
      name: en.services.remove.retryNamed.replace("{{name}}", PROVIDER.name),
    });
    expect(retry).toBe(action);
    expect(retry).toHaveFocus();
    expect(retry).toHaveTextContent(en.ds.action.retry);
  });

  it("opens the technical detail behind View details", async () => {
    const user = userEvent.setup();
    mount({
      error: new NativeError({
        code: "CONFIG_WRITE_FAILED",
        messageKey: "error.provider.removeFailed",
        technicalMessage: "Cannot delete the provider currently in use",
        remediation: "error.remediation.retryOrViewDetails",
        contextId: null,
      }),
    });
    const alert = screen.getByRole("alert", {
      name: en.services.remove.errorTitle,
    });
    const technical = within(alert).getByText(
      "Cannot delete the provider currently in use",
    );
    expect(technical).not.toBeVisible();

    await user.click(within(alert).getByText(en.error.details.show));

    expect(technical).toBeVisible();
    expect(alert).toHaveTextContent("CONFIG_WRITE_FAILED");
  });

  it("blocks Cancel, close, confirmation, and Escape while pending", async () => {
    const { onOpenChange } = mount({ busy: true });

    expect(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: en.services.remove.confirm }),
    ).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: en.ds.action.close }),
    ).toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it("blocks removal but keeps every safe exit available while actions are paused", async () => {
    const { onConfirm, onOpenChange } = mount({ mutationsBlocked: true });
    const dialog = screen.getByRole("dialog");

    expect(
      within(dialog).getByRole("alert", {
        name: "Service actions are paused",
      }),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", {
        name: en.services.remove.confirm,
      }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.close }),
    ).toBeEnabled();
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
