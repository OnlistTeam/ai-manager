import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { readStoredRoute, useAppRoute } from "@/app/useAppRoute";

describe("useAppRoute", () => {
  beforeEach(() => {
    window.localStorage.clear();
  });

  it("starts on the home page", () => {
    const { result } = renderHook(() => useAppRoute());
    expect(result.current.route).toBe("home");
  });

  it("remembers the page the user was last on", () => {
    const { result } = renderHook(() => useAppRoute());
    act(() => result.current.navigate("tools"));
    expect(result.current.route).toBe("tools");
    expect(readStoredRoute(window.localStorage)).toBe("tools");
  });

  it("still restores the last page on a normal launch", () => {
    window.localStorage.setItem("aimanager.route", "services");
    const { result } = renderHook(() => useAppRoute());
    expect(result.current.route).toBe("services");
  });

  it("falls back to home for pages that merged into a parent page", () => {
    for (const merged of ["routing", "usage", "sessions", "workspace"]) {
      window.localStorage.setItem("aimanager.route", merged);
      expect(readStoredRoute(window.localStorage)).toBe("home");
      const { result, unmount } = renderHook(() => useAppRoute());
      expect(result.current.route).toBe("home");
      unmount();
    }
  });

  it("falls back to home when the stored value is not a route", () => {
    window.localStorage.setItem("aimanager.route", "definitely-not-a-route");
    expect(readStoredRoute(window.localStorage)).toBe("home");
  });

  it("falls back to home when storage throws", () => {
    expect(
      readStoredRoute({
        getItem: () => {
          throw new Error("storage disabled");
        },
      }),
    ).toBe("home");
  });
});
