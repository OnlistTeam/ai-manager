import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { server } from "../msw/server";
import { native } from "@/native";

const TAURI_ENDPOINT = "http://tauri.local";

const CHOICE = {
  tool: "codex",
  model: "gpt-5.5",
  effort: "low",
  effortLevels: ["low", "medium", "high", "xhigh"],
  officialModels: ["gpt-5.5"],
  effortOverrides: [],
};

/** Home's model and effort choice (ADR-0054): only names cross IPC, never a path. */
describe("native.modelChoice", () => {
  it("sends the endpoint and the model, and reads back the choice", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_model_set`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json({ ...CHOICE, model: "gpt-6-sol" });
      }),
    );

    const choice = await native.modelChoice.setModel(
      "codex",
      "relay",
      "  gpt-6-sol ",
    );
    expect(seen).toEqual([
      { tool: "codex", provider: "relay", model: "gpt-6-sol" },
    ]);
    expect(choice.model).toBe("gpt-6-sol");
  });

  it("refuses a model name the backend would refuse, before sending it", async () => {
    let sent = false;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_model_set`, () => {
        sent = true;
        return HttpResponse.json(CHOICE);
      }),
    );
    expect(() =>
      native.modelChoice.setModel("codex", null, 'bad"name'),
    ).toThrow();
    expect(sent).toBe(false);
  });

  it("sends a reset to the tool default as null", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tool_effort_set`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({ ...CHOICE, effort: null });
        },
      ),
    );
    const choice = await native.modelChoice.setEffort("codex", null);
    expect(seen).toEqual([{ tool: "codex", effort: null }]);
    expect(choice.effort).toBeNull();
  });

  it("rejects a reply that carries anything beyond the choice", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_model_choice`, () =>
        HttpResponse.json({ ...CHOICE, path: "/Users/me/.codex/config.toml" }),
      ),
    );
    await expect(native.modelChoice.get("codex")).rejects.toThrow();
  });
});
