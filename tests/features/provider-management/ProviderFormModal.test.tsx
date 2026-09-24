import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { server } from "../../msw/server";
import en from "@/i18n/locales/en.json";
import { ProviderFormModal } from "@/features/provider-management";
import type { Provider, ProviderEditProfile } from "@/entities/provider";
import { NativeError } from "@/native";

const TAURI_ENDPOINT = "http://tauri.local";

const provider: Provider = {
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
};

const profile: ProviderEditProfile = {
  providerId: "relay",
  baseUrl: "https://relay.example.com/v1",
  endpointCandidates: ["https://relay-backup.example.com/v1"],
  endpointAutoSelect: true,
  models: ["model-a"],
  headerNames: ["Authorization"],
  capabilities: {
    canEditBaseUrl: true,
    canEditEndpoints: true,
    canEditModels: true,
    canEditHeaders: true,
    supportsMultipleModels: true,
  },
  baseUrlTakesNoVersion: false,
};

const singleModelProfile: ProviderEditProfile = {
  ...profile,
  headerNames: [],
  capabilities: {
    ...profile.capabilities,
    canEditHeaders: false,
    supportsMultipleModels: false,
  },
  baseUrlTakesNoVersion: false,
};

describe("ProviderFormModal", () => {
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

  it("opens on the current name with the stored key in plain text", () => {
    render(
      <ProviderFormModal
        provider={provider}
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByLabelText(en.services.form.name)).toHaveValue(
      "My Relay",
    );
    // The whole point is editing this exact string — without seeing it, the user can't verify it's the key they pasted.
    expect(within(dialog).getByLabelText(en.services.form.key)).toHaveValue(
      provider.apiKey,
    );
  });

  it("starts on the name instead of making keyboard users cross the dialog", () => {
    render(
      <ProviderFormModal
        provider={provider}
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    expect(screen.getByLabelText(en.services.form.name)).toHaveFocus();
  });

  it.each([true, false])(
    "copies the visible key and URL without submitting (route editor=%s)",
    async (canEditEndpoints) => {
      const user = userEvent.setup();
      const writeText = vi.spyOn(navigator.clipboard, "writeText");
      const onSubmit = vi.fn();
      render(
        <ProviderFormModal
          provider={provider}
          profile={{
            ...profile,
            capabilities: { ...profile.capabilities, canEditEndpoints },
          }}
          onOpenChange={vi.fn()}
          onSubmit={onSubmit}
        />,
      );
      const key = screen.getByLabelText(en.services.form.key);
      expect(key).toHaveAttribute("type", "text");
      expect(key).toHaveValue(provider.apiKey);
      const copyKey = screen.getByRole("button", {
        name: en.ds.action.copyNamed.replace("{{name}}", en.services.form.key),
      });
      copyKey.focus();
      await user.keyboard("{Enter}");
      expect(writeText).toHaveBeenLastCalledWith(provider.apiKey);
      const url = screen.getByLabelText(en.services.form.baseUrl);
      expect(url).toHaveValue(profile.baseUrl);
      await user.clear(url);
      await user.type(url, "https://changed.example/v1");
      await user.click(
        screen.getByRole("button", {
          name: en.ds.action.copyNamed.replace(
            "{{name}}",
            en.services.form.baseUrl,
          ),
        }),
      );
      expect(writeText).toHaveBeenLastCalledWith("https://changed.example/v1");
      if (canEditEndpoints) {
        await user.click(screen.getByText(en.services.form.routes));
        await user.click(
          screen.getByRole("button", {
            name: en.ds.action.copyNamed.replace(
              "{{name}}",
              profile.endpointCandidates[0]!,
            ),
          }),
        );
        expect(writeText).toHaveBeenLastCalledWith(
          profile.endpointCandidates[0],
        );
      }
      expect(onSubmit).not.toHaveBeenCalled();
    },
  );

  it("sends a null key when the key box was left alone", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const dialog = screen.getByRole("dialog");
    const name = within(dialog).getByLabelText(en.services.form.name);
    await userEvent.clear(name);
    await userEvent.type(name, "Renamed");
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).toHaveBeenCalledWith({ name: "Renamed", apiKey: null });
  });

  it("sends the typed key when the user replaced it", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const dialog = screen.getByRole("dialog");
    const key = within(dialog).getByLabelText(en.services.form.key);
    await userEvent.clear(key);
    await userEvent.type(key, "sk-new-123");
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      name: "My Relay",
      apiKey: "sk-new-123",
    });
  });

  it("shows Base URL directly and folds rarely used request headers", () => {
    render(
      <ProviderFormModal
        provider={provider}
        profile={profile}
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    expect(screen.getByLabelText("Model 1")).toHaveValue("model-a");
    expect(screen.getByLabelText(en.services.form.baseUrl)).toBeVisible();
    expect(
      screen.getByText(en.services.form.headers, { selector: "summary" }),
    ).toBeVisible();
    expect(screen.getByLabelText("Value for Authorization")).not.toBeVisible();
  });

  it("does not rewrite loaded settings when only the name or key is saved", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        profile={profile}
        advancedMode
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      name: "My Relay",
      apiKey: null,
    });
  });

  it("keeps a model draft when the same service profile refreshes", async () => {
    const view = render(
      <ProviderFormModal
        provider={provider}
        profile={singleModelProfile}
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    const model = screen.getByLabelText("Model 1");
    await userEvent.clear(model);
    await userEvent.type(model, "typed-model");
    view.rerender(
      <ProviderFormModal
        provider={provider}
        profile={{
          ...singleModelProfile,
          baseUrl: "https://refreshed.example",
        }}
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    expect(screen.getByLabelText("Model 1")).toHaveValue("typed-model");
  });

  it("offers a blank model field when a single-model tool has no override yet", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        profile={{ ...singleModelProfile, models: [] }}
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const model = screen.getByLabelText("Model 1");
    expect(model).toHaveValue("");
    await userEvent.type(model, "new-default-model");
    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      name: "My Relay",
      apiKey: null,
      models: ["new-default-model"],
    });
  });

  it("keeps stored header values write-only when replacing one", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        profile={profile}
        advancedMode
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    expect(screen.getByLabelText(en.services.form.baseUrl)).toHaveValue(
      "https://relay.example.com/v1",
    );
    const storedHeader = screen.getByLabelText("Value for Authorization");
    expect(storedHeader).toHaveAttribute("type", "text");
    expect(storedHeader).toHaveValue("");
    await userEvent.type(storedHeader, "replacement-secret");

    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      name: "My Relay",
      apiKey: null,
      advanced: {
        baseUrlChanged: false,
        baseUrl: null,
        endpointCandidates: null,
        endpointAutoSelect: null,
        headers: [{ name: "Authorization", value: "replacement-secret" }],
      },
    });
  });

  it("keeps alternate routes in the provider draft and lets a manual choice stay fixed", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        profile={profile}
        advancedMode
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );

    const backup = screen
      .getByText("https://relay-backup.example.com/v1")
      .closest("button");
    expect(backup).not.toBeNull();
    await userEvent.click(backup!);
    expect(screen.getByLabelText(en.services.form.baseUrl)).toHaveValue(
      "https://relay-backup.example.com/v1",
    );
    expect(
      screen.getByRole("checkbox", {
        name: new RegExp(en.services.form.routesAuto, "u"),
      }),
    ).not.toBeChecked();

    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      name: "My Relay",
      apiKey: null,
      advanced: {
        baseUrlChanged: true,
        baseUrl: "https://relay-backup.example.com/v1",
        endpointCandidates: [
          "https://relay-backup.example.com/v1",
          "https://relay.example.com/v1",
        ],
        endpointAutoSelect: false,
        headers: null,
      },
    });
  });

  it("tests only after an explicit action and auto-selects the fastest route", async () => {
    const requests: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_endpoints_test`,
        async ({ request }) => {
          requests.push(await request.json());
          return HttpResponse.json([
            {
              candidateId: "selected-route",
              latencyMs: 310,
              httpStatus: 200,
              failure: null,
            },
            {
              candidateId: "stored-route-0",
              latencyMs: 42,
              httpStatus: 204,
              failure: null,
            },
          ]);
        },
      ),
    );
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        profile={profile}
        advancedMode
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    expect(requests).toEqual([]);

    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.routesTest }),
    );
    expect(
      await screen.findByDisplayValue("https://relay-backup.example.com/v1"),
    ).toBeInTheDocument();
    expect(requests).toEqual([
      {
        tool: "claude-code",
        candidates: [
          { id: "selected-route", url: "https://relay.example.com/v1" },
          {
            id: "stored-route-0",
            url: "https://relay-backup.example.com/v1",
          },
        ],
      },
    ]);
    expect(JSON.stringify(requests)).not.toContain("apiKey");

    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        advanced: expect.objectContaining({
          baseUrl: "https://relay-backup.example.com/v1",
          endpointCandidates: [
            "https://relay-backup.example.com/v1",
            "https://relay.example.com/v1",
          ],
        }),
      }),
    );
  });

  it("keeps route edits local when the dialog is cancelled", async () => {
    const onOpenChange = vi.fn();
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        profile={profile}
        advancedMode
        onOpenChange={onOpenChange}
        onSubmit={onSubmit}
      />,
    );
    await userEvent.click(screen.getByText(en.services.form.routes));
    await userEvent.type(
      screen.getByLabelText(en.services.form.routeAddLabel),
      "https://third.example.com/v1",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.routeAdd }),
    );
    expect(screen.getByText("https://third.example.com/v1")).toBeVisible();

    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("requires a value for a newly added header", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        profile={profile}
        advancedMode
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    await userEvent.click(
      screen.getByText(en.services.form.headers, { selector: "summary" }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.addHeader }),
    );
    await userEvent.type(screen.getByLabelText("Header 2 name"), "X-Tenant");
    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText(en.services.form.headerRequired)).toBeVisible();

    await userEvent.type(
      screen.getByLabelText("Value for X-Tenant"),
      "tenant-a",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).toHaveBeenCalledWith(
      expect.objectContaining({
        advanced: expect.objectContaining({
          headers: [
            { name: "Authorization", value: null },
            { name: "X-Tenant", value: "tenant-a" },
          ],
        }),
      }),
    );
  });

  it("keeps a failed save safe and retryable inside the same form", async () => {
    const onSubmit = vi.fn();
    const onErrorReset = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        error={
          new NativeError({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.provider.saveFailed",
            technicalMessage: "/private/tool.json token=do-not-show",
            remediation: "error.remediation.checkServiceSettings",
            contextId: null,
          })
        }
        onOpenChange={vi.fn()}
        onErrorReset={onErrorReset}
        onSubmit={onSubmit}
      />,
    );

    const alert = screen.getByRole("alert", {
      name: "Could not finish saving this service",
    });
    expect(alert).toHaveTextContent(en.error.provider.saveFailed);
    expect(alert).toHaveTextContent(en.error.remediation.checkServiceSettings);
    expect(alert).toHaveTextContent(
      "AI Manager refreshed the saved service state. Review your changes, then try saving again.",
    );
    // Technical detail stays folded behind View details, never primary copy.
    expect(within(alert).getByText(/private\/tool\.json/)).not.toBeVisible();

    await userEvent.click(
      screen.getByRole("button", {
        name: "Try saving this service again",
      }),
    );
    expect(onSubmit).toHaveBeenCalledWith({
      name: "My Relay",
      apiKey: null,
    });
    await userEvent.type(screen.getByLabelText(en.services.form.name), " 2");
    expect(onErrorReset).toHaveBeenCalled();
  });

  it("refuses to submit an empty name without asking the backend", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const dialog = screen.getByRole("dialog");
    await userEvent.clear(within(dialog).getByLabelText(en.services.form.name));
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.form.save }),
    );
    expect(onSubmit).not.toHaveBeenCalled();
    expect(
      within(dialog).getByLabelText(en.services.form.name),
    ).toHaveAttribute("aria-invalid", "true");
    expect(
      within(dialog).getByText(en.services.form.nameRequired),
    ).toBeVisible();
  });

  it("submits with Enter from the form", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const name = screen.getByLabelText(en.services.form.name);
    name.focus();
    await userEvent.keyboard("{Enter}");
    expect(onSubmit).toHaveBeenCalledWith({
      name: "My Relay",
      apiKey: null,
    });
  });

  it("shows the name error when Enter submits an empty form", async () => {
    const onSubmit = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        onOpenChange={vi.fn()}
        onSubmit={onSubmit}
      />,
    );
    const name = screen.getByLabelText(en.services.form.name);
    await userEvent.clear(name);
    await userEvent.keyboard("{Enter}");
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByText(en.services.form.nameRequired)).toBeVisible();
  });

  it("shows the key in plain text so it can be compared against the source", () => {
    render(
      <ProviderFormModal
        provider={provider}
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    expect(screen.getByLabelText(en.services.form.key)).toHaveAttribute(
      "type",
      "text",
    );
  });

  it("tells assistive tech that a blank key box keeps the stored key", () => {
    render(
      <ProviderFormModal
        provider={provider}
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    // The one genuinely non-obvious rule in this form must reach a screen
    // reader through the field itself, not just as loose text nearby.
    expect(
      screen.getByLabelText(en.services.form.key),
    ).toHaveAccessibleDescription(
      en.services.form.keyHint.replace(
        "{{masked}}",
        "sk-ant-\u2022\u2022\u2022\u2022\u2022\u2022\u2022\u2022A12F",
      ),
    );
  });

  it("renders nothing when no service is selected", () => {
    render(
      <ProviderFormModal
        provider={null}
        onOpenChange={vi.fn()}
        onSubmit={vi.fn()}
      />,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("cannot be dismissed by Escape while saving", async () => {
    const onOpenChange = vi.fn();
    render(
      <ProviderFormModal
        provider={provider}
        busy
        onOpenChange={onOpenChange}
        onSubmit={vi.fn()}
      />,
    );
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});
