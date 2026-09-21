import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useRouteIntents } from "@/app/useRouteIntents";

describe("task-specific route handoffs", () => {
  it("opens the independent global prompt page for the selected software", () => {
    const navigate = vi.fn();
    const { result } = renderHook(() => useRouteIntents(navigate));
    act(() =>
      result.current.openExtensions({ kind: "tool", id: "codex" }, "prompt"),
    );
    expect(navigate).toHaveBeenLastCalledWith("prompts");
    expect(result.current.intents.extensionsTab).toEqual({
      kind: "tool",
      id: "codex",
    });
  });
  it("opens MCP for desktop configuration instead of landing in Skills", () => {
    const navigate = vi.fn();
    const { result } = renderHook(() => useRouteIntents(navigate));
    act(() =>
      result.current.openExtensions({
        kind: "desktopApp",
        id: "claude-desktop",
      }),
    );
    expect(navigate).toHaveBeenLastCalledWith("mcp");
  });
  it("keeps skill handoffs in the stable extensions route", () => {
    const navigate = vi.fn();
    const { result } = renderHook(() => useRouteIntents(navigate));
    act(() =>
      result.current.openExtensions({ kind: "tool", id: "codex" }, "skill"),
    );
    expect(navigate).toHaveBeenLastCalledWith("extensions");
  });
  it("relocates workspace handoffs to software details and clears the intent on normal navigation", () => {
    const navigate = vi.fn();
    const { result } = renderHook(() => useRouteIntents(navigate));
    act(() => result.current.openExtensions("workspace"));
    expect(navigate).toHaveBeenLastCalledWith("tools");
    expect(result.current.intents.toolDetails).toBe("openclaw");
    act(() => result.current.openRoute("home"));
    expect(result.current.intents.toolDetails).toBeNull();
  });
});
