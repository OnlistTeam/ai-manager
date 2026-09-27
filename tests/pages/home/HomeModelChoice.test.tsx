import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { HomePage } from "@/pages/home/HomePage";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

const CAPABILITIES = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: false,
  canLaunch: true,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: true,
  canChooseModel: true,
  canChooseEffort: true,
};

const CLAUDE = {
  id: "claude-code",
  name: "Claude Code",
  descriptionKey: "tool.claude-code.description",
  discovery: {
    publisher: "Anthropic",
    access: "vendorOrProvider",
    useCases: ["officialCoding"],
  },
  status: "installed",
  version: "2.1.0",
  latestVersion: null,
  capabilities: CAPABILITIES,
  sessionsInsideSettings: false,
  environment: null,
};

const GEMINI = {
  ...CLAUDE,
  id: "gemini-cli",
  name: "Gemini CLI",
  descriptionKey: "tool.gemini-cli.description",
  capabilities: { ...CAPABILITIES, canChooseEffort: false },
};

function provider(overrides: Record<string, unknown> = {}) {
  return {
    id: "relay",
    tool: "claude-code",
    name: "Relay",
    kind: "custom",
    active: false,
    baseUrl: "https://relay.example.test",
    apiKey: null,
    websiteUrl: null,
    testable: true,
    canRemove: true,
    ...overrides,
  };
}

function editProfile(providerId: string, models: string[]) {
  return {
    providerId,
    baseUrl: null,
    endpointCandidates: [],
    endpointAutoSelect: false,
    models,
    headerNames: [],
    capabilities: {
      canEditBaseUrl: true,
      canEditEndpoints: true,
      canEditModels: true,
      canEditHeaders: false,
      supportsMultipleModels: false,
    },
    baseUrlTakesNoVersion: false,
  };
}

/** The user's own Claude Code file: an effort, no model, one per-model level. */
const CHOICE = {
  tool: "claude-code",
  model: null,
  effort: "xhigh",
  effortLevels: ["low", "medium", "high", "xhigh"],
  officialModels: ["fable", "opus", "sonnet"],
  effortOverrides: [{ model: "claude-opus-5-5", effort: "medium" }],
};

interface Calls {
  models: unknown[];
  efforts: unknown[];
  activations: unknown[];
  catalogs: unknown[];
}

function serve(
  providers: Record<string, unknown[]>,
  choices: Record<string, Record<string, unknown>> = { "claude-code": CHOICE },
): Calls {
  const calls: Calls = {
    models: [],
    efforts: [],
    activations: [],
    catalogs: [],
  };
  const current = { ...choices };
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
      HttpResponse.json([CLAUDE, GEMINI]),
    ),
    http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
      HttpResponse.json({
        providers: [],
        configs: [],
        mcp: { total: 0, enabled: 0 },
      }),
    ),
    http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
      HttpResponse.json([]),
    ),
    http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
      HttpResponse.json(null, { status: 500 }),
    ),
    http.post(`${TAURI_ENDPOINT}/app_providers_list`, async ({ request }) => {
      const { tool } = (await request.json()) as { tool: string };
      return HttpResponse.json(providers[tool] ?? []);
    }),
    http.post(
      `${TAURI_ENDPOINT}/app_provider_edit_profile`,
      async ({ request }) => {
        const body = (await request.json()) as { provider: string };
        return HttpResponse.json(
          editProfile(
            body.provider,
            body.provider === "relay" ? ["glm-5"] : [],
          ),
        );
      },
    ),
    http.post(
      `${TAURI_ENDPOINT}/app_provider_models_list`,
      async ({ request }) => {
        calls.catalogs.push(await request.json());
        return HttpResponse.json({
          protocol: "anthropic",
          models: [
            { id: "glm-5", kind: "text" },
            { id: "glm-5-air", kind: "text" },
            { id: "cogview-4", kind: "image" },
          ],
          truncated: false,
          rejection: null,
        });
      },
    ),
    http.post(
      `${TAURI_ENDPOINT}/app_tool_model_choice`,
      async ({ request }) => {
        const { tool } = (await request.json()) as { tool: string };
        return HttpResponse.json(
          current[tool] ?? { ...CHOICE, tool, effortLevels: [], effort: null },
        );
      },
    ),
    http.post(`${TAURI_ENDPOINT}/app_tool_model_set`, async ({ request }) => {
      const body = (await request.json()) as { tool: string; model: string };
      calls.models.push(body);
      current[body.tool] = { ...current[body.tool], model: body.model };
      return HttpResponse.json(current[body.tool]);
    }),
    http.post(`${TAURI_ENDPOINT}/app_tool_effort_set`, async ({ request }) => {
      const body = (await request.json()) as { tool: string; effort: string };
      calls.efforts.push(body);
      current[body.tool] = { ...current[body.tool], effort: body.effort };
      return HttpResponse.json(current[body.tool]);
    }),
    http.post(
      `${TAURI_ENDPOINT}/app_provider_activation_prepare`,
      async ({ request }) => {
        const body = (await request.json()) as { provider: string };
        calls.activations.push(body);
        return HttpResponse.json({
          status: "notChecked",
          originProviderId: "official",
          activeProviderId: body.provider,
          providers: (providers["claude-code"] ?? []).map((entry) => ({
            ...(entry as Record<string, unknown>),
            active: (entry as { id: string }).id === body.provider,
          })),
          checks: [],
        });
      },
    ),
  );
  return calls;
}

function mount() {
  render(
    <HomePage
      onOpenTools={vi.fn()}
      onOpenServices={vi.fn()}
      onOpenMcp={vi.fn()}
    />,
    { wrapper: withQueryClient(createTestQueryClient()) },
  );
}

const OFFICIAL = provider({
  id: "official",
  name: "Claude Official",
  kind: "official",
  active: true,
  baseUrl: null,
});

async function modelPill(tool: string) {
  const row = await screen.findByRole("article", { name: tool });
  const pill = await within(row).findByRole("button", {
    name: new RegExp(`^Endpoint and model ${tool} uses: `),
  });
  await waitFor(() => expect(pill).toBeEnabled());
  return pill;
}

async function openModels(tool: string) {
  await userEvent.click(await modelPill(tool));
  return screen.findByRole("listbox", {
    name: `Endpoints and models for ${tool}`,
  });
}

const names = (listbox: HTMLElement) =>
  within(listbox)
    .getAllByRole("option")
    .map((option) => option.textContent);

describe("Home model and effort pickers", () => {
  beforeEach(async () => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn(),
    });
    toastMocks.error.mockClear();
    toastMocks.success.mockClear();
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        home: en.home,
        preferences: { check: en.preferences.check },
        services: en.services,
        tools: en.tools,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows the effort in the file and the tool default when no model is set", async () => {
    serve({ "claude-code": [OFFICIAL] });
    mount();

    const pill = await modelPill("Claude Code");
    expect(await within(pill).findByText("Claude Official")).toBeVisible();
    expect(await within(pill).findByText("Tool default")).toBeVisible();
    const row = await screen.findByRole("article", { name: "Claude Code" });
    expect(
      await within(row).findByRole("button", {
        name: "Thinking effort Claude Code uses: Extra high",
      }),
    ).toBeVisible();
  });

  it("groups models under their endpoint, the one in use first", async () => {
    const calls = serve({ "claude-code": [provider(), OFFICIAL] });
    mount();

    const listbox = await openModels("Claude Code");
    await waitFor(() =>
      expect(names(listbox)).toEqual([
        "Claude Official",
        "Tool default",
        "fable",
        "opus",
        "sonnet",
        "Relayglm-5",
        "glm-5",
        "glm-5-air",
        en.home.tools.manageEndpoints,
      ]),
    );
    // Only the custom endpoint's catalogue is read; the official one is built in.
    expect(calls.catalogs).toEqual([
      { tool: "claude-code", provider: "relay" },
    ]);
    const current = within(listbox).getByRole("option", {
      name: "Tool default",
    });
    expect(current).toHaveAttribute("aria-current", "true");
  });

  it("sets a model under the endpoint in use without switching", async () => {
    const calls = serve({ "claude-code": [OFFICIAL] });
    mount();

    const listbox = await openModels("Claude Code");
    await userEvent.click(
      within(listbox).getByRole("option", { name: "opus" }),
    );

    await waitFor(() =>
      expect(calls.models).toEqual([
        { tool: "claude-code", provider: "official", model: "opus" },
      ]),
    );
    expect(calls.activations).toEqual([]);
    expect(
      await within(await modelPill("Claude Code")).findByText("opus"),
    ).toBeVisible();
  });

  it("switches first, then sets the model picked under another endpoint", async () => {
    const calls = serve({ "claude-code": [OFFICIAL, provider()] });
    mount();

    const listbox = await openModels("Claude Code");
    const option = await within(listbox).findByRole("option", {
      name: "glm-5-air",
    });
    await userEvent.click(option);

    await waitFor(() =>
      expect(calls.models).toEqual([
        { tool: "claude-code", provider: "relay", model: "glm-5-air" },
      ]),
    );
    expect(calls.activations).toEqual([
      { tool: "claude-code", provider: "relay" },
    ]);
  });

  it("uses a model name typed into the filter", async () => {
    const calls = serve({ "claude-code": [OFFICIAL] });
    mount();

    await openModels("Claude Code");
    const user = userEvent.setup();
    await user.type(
      screen.getByPlaceholderText(en.home.model.filter),
      "claude-opus-5-5-preview",
    );
    await user.click(
      screen.getByRole("option", { name: "Use “claude-opus-5-5-preview”" }),
    );

    await waitFor(() =>
      expect(calls.models).toEqual([
        {
          tool: "claude-code",
          provider: "official",
          model: "claude-opus-5-5-preview",
        },
      ]),
    );
  });

  it("chooses an effort, names the models that keep their own, and resets to the default", async () => {
    const calls = serve({ "claude-code": [OFFICIAL] });
    mount();

    const row = await screen.findByRole("article", { name: "Claude Code" });
    const user = userEvent.setup();
    await user.click(
      await within(row).findByRole("button", {
        name: /^Thinking effort Claude Code uses: /,
      }),
    );
    const listbox = await screen.findByRole("listbox", {
      name: "Thinking effort for Claude Code",
    });
    expect(names(listbox)).toEqual([
      "Tool default",
      "Lowlow",
      "Mediummedium",
      "Highhigh",
      "Extra highxhigh",
    ]);
    expect(
      screen.getByText(/claude-opus-5-5 Medium/, { exact: false }),
    ).toBeVisible();
    await user.click(within(listbox).getByRole("option", { name: /^High/ }));
    await waitFor(() =>
      expect(calls.efforts).toEqual([{ tool: "claude-code", effort: "high" }]),
    );

    await user.click(
      await within(row).findByRole("button", {
        name: "Thinking effort Claude Code uses: High",
      }),
    );
    await user.click(
      await screen.findByRole("option", { name: "Tool default" }),
    );
    await waitFor(() =>
      expect(calls.efforts).toEqual([
        { tool: "claude-code", effort: "high" },
        { tool: "claude-code", effort: null },
      ]),
    );
  });

  it("keeps the effort slot empty for a tool without an effort setting", async () => {
    serve(
      { "claude-code": [OFFICIAL] },
      {
        "claude-code": CHOICE,
        "gemini-cli": {
          ...CHOICE,
          tool: "gemini-cli",
          effort: null,
          effortLevels: [],
          officialModels: ["auto", "pro"],
          effortOverrides: [],
        },
      },
    );
    mount();

    const row = await screen.findByRole("article", { name: "Gemini CLI" });
    await modelPill("Gemini CLI");
    expect(
      within(row).queryByRole("button", { name: /Thinking effort/ }),
    ).toBeNull();
    const listbox = await openModels("Gemini CLI");
    expect(names(listbox)).toEqual([
      en.home.tools.official,
      "Tool default",
      "auto",
      "pro",
      en.home.tools.addEndpoint,
    ]);
  });
});
