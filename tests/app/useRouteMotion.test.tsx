import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import type { AppRoute } from "@/app/routes";
import { useRouteMotion } from "@/app/useRouteMotion";

describe("useRouteMotion", () => {
  it("slides in the same direction as the navigation order", () => {
    const navigate = vi.fn();
    const { result, rerender } = renderHook(
      ({ route }: { route: AppRoute }) => useRouteMotion(route, navigate),
      { initialProps: { route: "home" as AppRoute } },
    );

    act(() => result.current.navigate("services"));
    expect(result.current.direction).toBe("forward");
    expect(navigate).toHaveBeenLastCalledWith("services");

    rerender({ route: "services" });
    act(() => result.current.navigate("home"));
    expect(result.current.direction).toBe("backward");
    expect(navigate).toHaveBeenLastCalledWith("home");
  });
});
