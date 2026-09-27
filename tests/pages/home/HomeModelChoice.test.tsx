import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
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

/**
 * A real Claude Code file: no model, the older tool-wide level, and levels
 * `/effort` saved per model that disagree.
 */
const CHOICE = {
  tool: "claude-code",
  model: null,
  effort: {
    kind: "mixed",
    perModel: [
      { model: "claude-fable-5-1", effort: "xhigh", modelDefault: false },
      { model: "claude-opus-5-5", effort: "xhigh", modelDefault: false },
      { model: "claude-opus-5", effort: "high", modelDefault: false },
      { model: "claude-sonnet-5", effort: "medium", modelDefault: true },
    ],
  },
  effortLevels: ["low", "medium", "high", "xhigh"],
  officialModels: ["fable", "opus", "sonnet"],
};

/** What the backend answers after a write, as Claude Code will resolve it. */
function effortAfter(effort: string | null) {
  return effort === null
    ? { kind: "toolDefault" }
    : { kind: "level", level: effort };
}

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
          current[tool] ?? {
            ...CHOICE,
            tool,
            effortLevels: [],
            effort: { kind: "toolDefault" },
          },
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
      const body = (await request.json()) as {
        tool: string;
        effort: string | null;
      };
      calls.efforts.push(body);
      current[body.tool] = {
        ...current[body.tool],
        effort: effortAfter(body.effort),
      };
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
    name: new RegExp(`^Model ${tool} uses: `),
  });
  await waitFor(() => expect(pill).toBeEnabled());
  return pill;
}

async function openModels(tool: string) {
  await userEvent.click(await modelPill(tool));
  return screen.findByRole("listbox", {
    name: `Models for ${tool}`,
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

  it("says the models differ when no model is set and their levels disagree", async () => {
    serve({ "claude-code": [OFFICIAL] });
    mount();

    const pill = await modelPill("Claude Code");
    expect(pill).toHaveAccessibleName(
      "Model Claude Code uses: Default, Claude Official",
    );
    const row = await screen.findByRole("article", { name: "Claude Code" });
    expect(
      await within(row).findByRole("button", {
        name: "Thinking effort Claude Code uses: Per model",
      }),
    ).toBeVisible();
  });

  it("lists only the models of the endpoint in use", async () => {
    const calls = serve({ "claude-code": [OFFICIAL, provider()] });
    mount();

    const listbox = await openModels("Claude Code");
    expect(names(listbox)).toEqual([
      "DefaultLet Claude Code choose",
      "fable",
      "opus",
      "sonnet",
      en.home.tools.manageEndpoints,
    ]);
    // The official endpoint's list is built in; no other endpoint is read.
    expect(calls.catalogs).toEqual([]);
    const current = within(listbox).getByRole("option", {
      name: "Default Let Claude Code choose",
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

  it("lists a custom endpoint's own catalogue and sets a model from it", async () => {
    const calls = serve({
      "claude-code": [
        { ...OFFICIAL, active: false },
        provider({ active: true }),
      ],
    });
    mount();

    const listbox = await openModels("Claude Code");
    await waitFor(() =>
      expect(names(listbox)).toEqual([
        "DefaultLet Claude Code choose",
        "glm-5",
        "glm-5-air",
        en.home.tools.manageEndpoints,
      ]),
    );
    expect(calls.catalogs).toEqual([
      { tool: "claude-code", provider: "relay" },
    ]);
    await userEvent.click(
      within(listbox).getByRole("option", { name: "glm-5-air" }),
    );

    await waitFor(() =>
      expect(calls.models).toEqual([
        { tool: "claude-code", provider: "relay", model: "glm-5-air" },
      ]),
    );
    expect(calls.activations).toEqual([]);

    // The catalogue is kept, so opening the list again reads nothing.
    await waitFor(async () =>
      expect(await modelPill("Claude Code")).not.toBeDisabled(),
    );
    const again = await openModels("Claude Code");
    expect(names(again)).toContain("glm-5");
    expect(calls.catalogs).toHaveLength(1);
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

  it("moves the effort slider to set a level for every model", async () => {
    const calls = serve({ "claude-code": [OFFICIAL] });
    mount();

    const row = await screen.findByRole("article", { name: "Claude Code" });
    const user = userEvent.setup();
    const pill = await within(row).findByRole("button", {
      name: "Thinking effort Claude Code uses: Per model",
    });
    expect(pill).toHaveAttribute(
      "title",
      "claude-fable-5-1 Extra high\nclaude-opus-5-5 Extra high\nclaude-opus-5 High\nclaude-sonnet-5 Medium",
    );
    await user.click(pill);
    const slider = await screen.findByRole("slider", {
      name: "Thinking effort",
    });
    // Levels only: no default stop, and Max only persists through a variable,
    // which reaches running sessions.
    expect(slider).toHaveAttribute("max", "3");
    expect(screen.getByText("Low")).toBeVisible();
    expect(screen.getByText("Extra high")).toBeVisible();
    // The models differ, so the thumb stands on no level.
    expect(slider).toHaveAttribute("aria-valuetext", "Per model");

    fireEvent.change(slider, { target: { value: "2" } });
    await waitFor(() =>
      expect(calls.efforts).toEqual([{ tool: "claude-code", effort: "high" }]),
    );
    fireEvent.change(slider, { target: { value: "0" } });
    await waitFor(() =>
      expect(calls.efforts).toEqual([
        { tool: "claude-code", effort: "high" },
        { tool: "claude-code", effort: "low" },
      ]),
    );
    expect(
      await within(row).findByRole("button", {
        name: "Thinking effort Claude Code uses: Low",
      }),
    ).toBeVisible();
  });

  it("writes the weakest level when it is clicked while no level is placed", async () => {
    const calls = serve({ "claude-code": [OFFICIAL] });
    mount();

    const row = await screen.findByRole("article", { name: "Claude Code" });
    await userEvent.click(
      await within(row).findByRole("button", {
        name: "Thinking effort Claude Code uses: Per model",
      }),
    );
    const slider = await screen.findByRole("slider");
    // The hidden thumb already sits on the first stop, so no `change` fires.
    fireEvent.pointerUp(slider);
    await waitFor(() =>
      expect(calls.efforts).toEqual([{ tool: "claude-code", effort: "low" }]),
    );
  });

  it("writes nothing while the slider is dragged, then the stop it is let go on", async () => {
    const calls = serve({ "claude-code": [OFFICIAL] });
    mount();

    const row = await screen.findByRole("article", { name: "Claude Code" });
    await userEvent.click(
      await within(row).findByRole("button", {
        name: /^Thinking effort Claude Code uses: /,
      }),
    );
    const slider = await screen.findByRole("slider");
    for (const stop of ["0", "1", "2"]) {
      fireEvent.input(slider, { target: { value: stop } });
    }
    expect(slider).toHaveAttribute("aria-valuetext", "High");
    await new Promise((resolve) => setTimeout(resolve, 400));
    expect(calls.efforts).toEqual([]);

    fireEvent.change(slider, { target: { value: "1" } });
    await waitFor(() =>
      expect(calls.efforts).toEqual([
        { tool: "claude-code", effort: "medium" },
      ]),
    );
  });

  it("writes only the stop quick key presses settle on", async () => {
    const calls = serve({ "claude-code": [OFFICIAL] });
    mount();

    const row = await screen.findByRole("article", { name: "Claude Code" });
    await userEvent.click(
      await within(row).findByRole("button", {
        name: /^Thinking effort Claude Code uses: /,
      }),
    );
    const slider = await screen.findByRole("slider");
    for (const stop of ["0", "1", "2", "3"]) {
      fireEvent.change(slider, { target: { value: stop } });
    }
    await waitFor(() =>
      expect(calls.efforts).toEqual([{ tool: "claude-code", effort: "xhigh" }]),
    );
  });

  it("shows a level a terminal variable holds and cannot move it", async () => {
    serve(
      { "claude-code": [OFFICIAL] },
      {
        "claude-code": {
          ...CHOICE,
          effort: {
            kind: "terminal",
            level: "max",
            source: {
              kind: "shellFile",
              variable: "CLAUDE_CODE_EFFORT_LEVEL",
              path: "~/.zshrc:12",
            },
          },
        },
      },
    );
    mount();

    const row = await screen.findByRole("article", { name: "Claude Code" });
    await userEvent.click(
      await within(row).findByRole("button", {
        name: "Thinking effort Claude Code uses: Max",
      }),
    );
    expect(await screen.findByRole("slider")).toBeDisabled();
    expect(
      screen.getByText(
        "Set by the terminal variable CLAUDE_CODE_EFFORT_LEVEL in ~/.zshrc:12; it cannot be changed here.",
      ),
    ).toBeVisible();
  });

  it("keeps the effort slot empty for a tool without an effort setting", async () => {
    serve(
      { "claude-code": [OFFICIAL] },
      {
        "claude-code": CHOICE,
        "gemini-cli": {
          ...CHOICE,
          tool: "gemini-cli",
          effort: { kind: "toolDefault" },
          effortLevels: [],
          officialModels: ["auto", "pro"],
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
      "DefaultLet Gemini CLI choose",
      "auto",
      "pro",
      en.home.tools.manageEndpoints,
    ]);
  });
});
