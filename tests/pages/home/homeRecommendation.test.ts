import { describe, expect, it } from "vitest";
import type {
  QuickCheckItem,
  QuickCheckItemKind,
  QuickCheckItemStatus,
  QuickCheckSummary,
} from "@/features/health";
import type { ToolId } from "@/entities/tool";
import { recommendHomeAction } from "@/pages/home/homeRecommendation";

function item(
  id: string,
  kind: QuickCheckItemKind,
  status: QuickCheckItemStatus,
  toolId: ToolId | null = null,
): QuickCheckItem {
  return {
    id,
    kind,
    status,
    titleKey: `title.${id}`,
    descriptionKey: `description.${id}`,
    values: { name: id },
    toolId,
  };
}

function summary(
  items: QuickCheckItem[],
  installedCount = 1,
): QuickCheckSummary {
  return {
    status: items.some((entry) => entry.status === "action")
      ? "action"
      : items.some((entry) => entry.status === "attention") ||
          installedCount === 0
        ? "attention"
        : "ready",
    attentionCount: items.filter(
      (entry) => entry.status === "action" || entry.status === "attention",
    ).length,
    installedCount,
    updatableCount: 0,
    configuredServiceCount: 0,
    mcpTotalCount: 0,
    mcpEnabledCount: 0,
    items,
  };
}

describe("recommendHomeAction", () => {
  it("does not invent a recommendation when no tool is installed", () => {
    expect(
      recommendHomeAction(summary([item("claude", "tool", "info")], 0)),
    ).toBeNull();
  });

  it("does not turn ready or informational rows into work", () => {
    expect(
      recommendHomeAction(
        summary([item("claude", "tool", "ready"), item("mcp", "mcp", "info")]),
      ),
    ).toBeNull();
  });

  it("sends a broken tool to AI Tools ahead of lower-severity warnings", () => {
    const result = recommendHomeAction(
      summary([
        item("provider", "provider", "attention"),
        item("tool", "tool", "action"),
      ]),
    );

    expect(result).toMatchObject({
      item: { id: "tool" },
      destination: "tools",
      actionKey: "home.card.actions.tools",
    });
  });

  it("prioritizes a missing service over an optional tool update", () => {
    const result = recommendHomeAction(
      summary([
        item("update", "tool", "attention"),
        item("provider", "provider", "attention", "claude-code"),
      ]),
    );

    expect(result).toMatchObject({
      item: { id: "provider" },
      destination: "services",
      actionKey: "home.card.actions.services",
      toolId: "claude-code",
    });
  });

  it("routes connection trouble to AI Services", () => {
    expect(
      recommendHomeAction(
        summary([item("connection", "connectivity", "attention", "codex")]),
      ),
    ).toMatchObject({ destination: "services", toolId: "codex" });
  });

  it("routes an unreadable configuration to AI Services", () => {
    expect(
      recommendHomeAction(
        summary([item("config", "config", "action", "gemini-cli")]),
      ),
    ).toMatchObject({
      destination: "services",
      actionKey: "home.card.actions.services",
      toolId: "gemini-cli",
    });
  });

  it("routes MCP inventory trouble to Skills & MCP", () => {
    expect(
      recommendHomeAction(summary([item("mcp", "mcp", "attention")])),
    ).toMatchObject({
      destination: "extensions",
      actionKey: "home.card.actions.extensions",
    });
  });
});
