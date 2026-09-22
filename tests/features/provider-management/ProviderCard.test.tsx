import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { ProviderCard } from "@/features/provider-management";
import type { Provider } from "@/entities/provider";
import { NativeError } from "@/native";

function provider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: "relay",
    tool: "claude-code",
    name: "My Relay",
    kind: "custom",
    active: false,
    baseUrl: "https://relay.example.com",
    apiKey: "sk-ant-api03-EXAMPLE-NOT-A-REAL-KEY-A12F",
    websiteUrl: null,
    testable: true,
    canRemove: true,
    ...overrides,
  };
}

describe("ProviderCard", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, services: en.services, tool: en.tool },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows the key in full and offers to copy it and the endpoint", () => {
    const subject = provider();
    render(
      <ProviderCard
        provider={subject}
        toolName="Claude Code"
        effectiveState="saved"
      />,
    );
    expect(screen.getByText(subject.apiKey ?? "")).toBeInTheDocument();
    for (const name of [en.services.card.savedEndpoint, en.services.form.key]) {
      expect(
        screen.getByRole("button", {
          name: en.ds.action.copyNamed.replace("{{name}}", name),
        }),
      ).toBeInTheDocument();
    }
  });

  it("says which tool uses it once it is the active one", () => {
    render(
      <ProviderCard
        provider={provider({ active: true })}
        toolName="Claude Code"
        effectiveState="inUse"
      />,
    );
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
    expect(screen.getByText(en.services.card.inUse)).toBeInTheDocument();
  });

  it("shows recent use and tool-stored credentials without claiming an active session", async () => {
    const onUse = vi.fn();
    render(
      <ProviderCard
        provider={provider({ tool: "opencode", additive: true, apiKey: null })}
        toolName="OpenCode"
        effectiveState="recentModel"
        effectiveCredential="configured"
        onUse={onUse}
      />,
    );
    expect(screen.getByText(en.services.card.recentModel)).toBeInTheDocument();
    expect(screen.queryByText(en.services.card.inUse)).not.toBeInTheDocument();
    expect(
      screen.getByText(en.services.effective.credential.configured),
    ).toBeInTheDocument();
    expect(screen.queryByText(en.services.card.noKey)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: en.ds.action.copyNamed.replace("{{name}}", en.services.form.key),
      }),
    ).not.toBeInTheDocument();
    await userEvent
      .setup()
      .click(screen.getByRole("button", { name: "Add My Relay to tool" }));
    expect(onUse).toHaveBeenCalledOnce();
  });

  it.each(["unknown", "defaultModel"] as const)(
    "renders an explicit %s label",
    (state) => {
      render(
        <ProviderCard
          provider={provider({ apiKey: null })}
          toolName="OpenCode"
          effectiveState={state}
          effectiveCredential="unknown"
        />,
      );
      expect(screen.getByText(en.services.card[state])).toBeInTheDocument();
      expect(
        screen.getByText(en.services.effective.credential.unknown),
      ).toBeInTheDocument();
      expect(
        screen.queryByText(en.services.card.inUse),
      ).not.toBeInTheDocument();
    },
  );

  it("shows the source note for the card in effect when it is given one", () => {
    render(
      <ProviderCard
        provider={provider({ active: true })}
        toolName="Claude Code"
        effectiveState="inUse"
        sourceNote="From ANTHROPIC_BASE_URL in ~/.config/zsh/secrets.zsh:3"
      />,
    );
    expect(
      screen.getByText(
        "From ANTHROPIC_BASE_URL in ~/.config/zsh/secrets.zsh:3",
      ),
    ).toBeInTheDocument();
  });

  it.each(["configured", "unknown", "missing", "toolLogin"] as const)(
    "keeps saved keys visible and copyable despite %s runtime evidence",
    async (credential) => {
      const user = userEvent.setup();
      const writeText = vi.spyOn(navigator.clipboard, "writeText");
      const subject = provider();
      render(
        <ProviderCard
          provider={subject}
          toolName="Claude Code"
          effectiveState="inUse"
          effectiveCredential={credential}
        />,
      );
      expect(screen.getByText(subject.apiKey!)).toBeVisible();
      await user.click(
        screen.getByRole("button", {
          name: en.ds.action.copyNamed.replace(
            "{{name}}",
            en.services.form.key,
          ),
        }),
      );
      expect(writeText).toHaveBeenLastCalledWith(subject.apiKey);
      await user.click(
        screen.getByRole("button", {
          name: en.ds.action.copyNamed.replace(
            "{{name}}",
            en.services.card.savedEndpoint,
          ),
        }),
      );
      expect(writeText).toHaveBeenLastCalledWith(subject.baseUrl);
      expect(screen.getByText(en.services.card.inUse)).toBeVisible();
    },
  );

  it("omits the source note when none is given", () => {
    render(
      <ProviderCard
        provider={provider({ active: true })}
        toolName="Claude Code"
        effectiveState="inUse"
      />,
    );
    expect(screen.queryByText(/^From /)).not.toBeInTheDocument();
  });

  it("shows who overrides a selected but ineffective service, and still offers Use to re-apply it", () => {
    render(
      <ProviderCard
        provider={{ ...provider(), active: true }}
        toolName="Claude Code"
        effectiveState="overridden"
        overrideSource="ANTHROPIC_BASE_URL"
      />,
    );
    expect(
      screen.getByText("Selected, but overridden by ANTHROPIC_BASE_URL"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Use/ })).toBeInTheDocument();
  });

  it("offers Use on a card in effect but not the DB selection", () => {
    render(
      <ProviderCard
        provider={provider({ active: false })}
        toolName="Claude Code"
        effectiveState="inUse"
      />,
    );
    expect(screen.getByRole("button", { name: /Use/ })).toBeInTheDocument();
  });

  it("hides Use on the card that is both in effect and the DB selection", () => {
    render(
      <ProviderCard
        provider={provider({ active: true })}
        toolName="Claude Code"
        effectiveState="inUse"
      />,
    );
    expect(
      screen.queryByRole("button", { name: /Use/ }),
    ).not.toBeInTheDocument();
  });

  it("offers Use when it is not the active one", async () => {
    const onUse = vi.fn();
    render(
      <ProviderCard
        provider={provider()}
        toolName="Claude Code"
        effectiveState="saved"
        onUse={onUse}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.useNamed.replace("{{name}}", "My Relay"),
      }),
    );
    expect(onUse).toHaveBeenCalledTimes(1);
  });

  it("keeps a partially applied switch safe and retryable on the same card", async () => {
    const onUse = vi.fn();
    render(
      <ProviderCard
        provider={provider({ active: true, canRemove: false })}
        toolName="Claude Code"
        effectiveState="saved"
        switchError={
          new NativeError({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.provider.switchFailed",
            technicalMessage: "/private/tool.json token=do-not-show",
            remediation: "error.remediation.checkPermissions",
            contextId: null,
          })
        }
        onUse={onUse}
      />,
    );

    const alert = screen.getByRole("alert", {
      name: "Could not finish switching to My Relay",
    });
    expect(alert).toHaveTextContent(en.error.provider.switchFailed);
    expect(alert).toHaveTextContent(en.error.remediation.checkPermissions);
    expect(alert).not.toHaveTextContent("/private/tool.json");
    expect(alert).not.toHaveTextContent("do-not-show");
    await userEvent.click(
      screen.getByRole("button", {
        name: "Try switching to My Relay again",
      }),
    );
    expect(onUse).toHaveBeenCalledTimes(1);
  });

  it("offers Remove for an inactive standard service", async () => {
    const onRemove = vi.fn();
    render(
      <ProviderCard
        provider={provider()}
        toolName="Claude Code"
        effectiveState="saved"
        onRemove={onRemove}
      />,
    );

    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.action.removeNamed.replace("{{name}}", "My Relay"),
      }),
    );
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it("keeps the reason the active service cannot be removed off the card face", () => {
    render(
      <ProviderCard
        provider={provider({ active: true, canRemove: false })}
        toolName="Claude Code"
        effectiveState="saved"
      />,
    );

    const remove = screen.getByRole("button", {
      name: en.services.action.removeNamed.replace("{{name}}", "My Relay"),
    });
    expect(remove).toBeDisabled();
    // The reason is still there, but conveyed via a hover hint and aria-describedby, no longer taking up a permanent line.
    const hint = screen.getByText(en.services.remove.activeHint);
    expect(hint).toHaveClass("sr-only");
    expect(remove).toHaveAttribute("aria-describedby", hint.id);
  });

  it("fails closed with a reachable reason for tool-managed services", () => {
    render(
      <ProviderCard
        provider={provider({ canRemove: false })}
        toolName="OpenCode"
        effectiveState="saved"
      />,
    );

    const remove = screen.getByRole("button", {
      name: en.services.action.removeNamed.replace("{{name}}", "My Relay"),
    });
    expect(remove).toBeDisabled();
    expect(remove).toHaveAttribute(
      "aria-describedby",
      screen.getByText(en.services.remove.managedHint).id,
    );
  });

  it("hides the check button for a service with nothing to check", () => {
    render(
      <ProviderCard
        provider={provider({ baseUrl: null, apiKey: null, testable: false })}
        toolName="Claude Code"
        effectiveState="saved"
      />,
    );
    expect(
      screen.queryByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "My Relay"),
      }),
    ).not.toBeInTheDocument();
  });

  it("reports the outcome of a finished check", () => {
    render(
      <ProviderCard
        provider={provider()}
        toolName="Claude Code"
        effectiveState="saved"
        testResult={{
          providerId: "relay",
          reachability: "operational",
          responseTimeMs: 240,
          httpStatus: 200,
        }}
      />,
    );
    expect(screen.getByText(en.services.test.operational)).toBeInTheDocument();
  });

  it("guides a failed official check to compatible services", async () => {
    const onBrowseCompatible = vi.fn();
    render(
      <ProviderCard
        provider={provider({
          name: "Anthropic Official",
          kind: "official",
          baseUrl: null,
          apiKey: null,
          testable: true,
        })}
        toolName="Claude Code"
        effectiveState="saved"
        testResult={{
          providerId: "relay",
          reachability: "failed",
          responseTimeMs: null,
          httpStatus: null,
        }}
        onBrowseCompatible={onBrowseCompatible}
      />,
    );

    expect(screen.getByText(en.services.test.failed)).toBeInTheDocument();
    expect(screen.getByText(en.services.test.unreachableHint)).toBeVisible();
    await userEvent.click(
      screen.getByRole("button", {
        name: en.services.test.browseCompatibleNamed.replace(
          "{{name}}",
          "Anthropic Official",
        ),
      }),
    );
    expect(onBrowseCompatible).toHaveBeenCalledTimes(1);
  });

  it("keeps a failed check safe and retryable on the same card", async () => {
    const onTest = vi.fn();
    render(
      <ProviderCard
        provider={provider()}
        toolName="Claude Code"
        effectiveState="saved"
        testError={
          new NativeError({
            code: "PROVIDER_UNREACHABLE",
            messageKey: "error.provider.testFailed",
            technicalMessage:
              "connection refused at 10.0.0.1 token=do-not-show",
            remediation: "error.remediation.checkServiceSettings",
            contextId: null,
          })
        }
        onTest={onTest}
      />,
    );

    const alert = screen.getByRole("alert", {
      name: "Could not check the address for My Relay",
    });
    expect(alert).toHaveTextContent(en.error.provider.testFailed);
    expect(alert).toHaveTextContent(en.error.remediation.checkServiceSettings);
    expect(alert).not.toHaveTextContent("10.0.0.1");
    expect(alert).not.toHaveTextContent("do-not-show");

    const retry = screen.getByRole("button", {
      name: "Check My Relay again",
    });
    expect(retry).toHaveTextContent("Try again");
    await userEvent.click(retry);
    expect(onTest).toHaveBeenCalledTimes(1);
  });

  it("disables its buttons while something is running", () => {
    render(
      <ProviderCard
        provider={provider()}
        toolName="Claude Code"
        effectiveState="saved"
        busy
      />,
    );
    expect(
      screen.getByRole("button", {
        name: en.services.action.useNamed.replace("{{name}}", "My Relay"),
      }),
    ).toBeDisabled();
  });

  it("labels official sign-in honestly instead of warning about a missing key", () => {
    render(
      <ProviderCard
        provider={provider({
          kind: "official",
          apiKey: null,
          testable: true,
        })}
        toolName="Claude Code"
        effectiveState="saved"
      />,
    );
    expect(screen.getByText(en.services.card.official)).toBeInTheDocument();
    expect(
      screen.getByText(
        en.services.card.toolSignIn.replace("{{tool}}", "Claude Code"),
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText(en.services.card.noKey)).not.toBeInTheDocument();
  });

  it("gives Check, Edit, and Remove unique names when cards repeat actions", () => {
    render(
      <ProviderCard
        provider={provider()}
        toolName="Claude Code"
        effectiveState="saved"
      />,
    );
    expect(
      screen.getByRole("button", {
        name: en.services.action.testNamed.replace("{{name}}", "My Relay"),
      }),
    ).toHaveTextContent(en.services.action.test);
    expect(
      screen.getByRole("button", {
        name: en.services.action.editNamed.replace("{{name}}", "My Relay"),
      }),
    ).toHaveTextContent(en.services.action.edit);
    expect(
      screen.getByRole("button", {
        name: en.services.action.removeNamed.replace("{{name}}", "My Relay"),
      }),
    ).toHaveTextContent(en.services.action.remove);
  });
});

/**
 * A tool that keeps its own credential store leaves the saved record with no
 * key at all, so the row reads "Key configured" with nothing to copy. Without
 * a line saying where the key actually is, that reads as the card failing to
 * show something it has.
 */
describe("a key this app does not hold", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, services: en.services, tool: en.tool },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("names where the key lives instead of only saying one exists", () => {
    render(
      <ProviderCard
        provider={provider({ apiKey: null })}
        toolName="OpenCode"
        effectiveState="inUse"
        effectiveCredential="configured"
        credentialNote="From ~/.local/share/opencode/auth.json"
      />,
    );

    expect(
      screen.getByText(en.services.effective.credential.configured),
    ).toBeVisible();
    expect(
      screen.getByText("From ~/.local/share/opencode/auth.json"),
    ).toBeVisible();
  });

  /** A key this app does hold is shown in full, and needs no such note. */
  it("says nothing extra when the key is the saved one", () => {
    render(
      <ProviderCard
        provider={provider()}
        toolName="Claude Code"
        effectiveState="saved"
      />,
    );
    expect(screen.queryByText(/^From /)).toBeNull();
  });
});
