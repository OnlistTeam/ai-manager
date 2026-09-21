import { render, screen, waitFor } from "@testing-library/react";
import { toast } from "sonner";
import { afterEach, describe, expect, it } from "vitest";
import { Toaster } from "@/shared/ui/Toaster";

afterEach(() => {
  toast.dismiss();
});

describe("Toaster", () => {
  it("mounts a polite live region for notifications", () => {
    render(<Toaster />);
    expect(screen.getByLabelText(/Notifications/)).toHaveAttribute(
      "aria-live",
      "polite",
    );
  });

  it("passes the app theme through to sonner", async () => {
    render(<Toaster theme="dark" />);
    toast.success("Ready");
    await waitFor(() => expect(screen.getByText("Ready")).toBeInTheDocument());
    expect(document.querySelector("[data-sonner-toaster]")).toHaveAttribute(
      "data-sonner-theme",
      "dark",
    );
  });

  it("themes sonner's own stylesheet via --normal-* CSS variables, since it outranks utility classes", async () => {
    render(<Toaster />);
    toast.success("Ready");
    await waitFor(() => expect(screen.getByText("Ready")).toBeInTheDocument());
    const toaster = document.querySelector("[data-sonner-toaster]");
    const style = toaster?.getAttribute("style") ?? "";
    expect(style).toContain("--normal-bg: hsl(var(--ui-surface))");
    expect(style).toContain("--normal-border: hsl(var(--ui-border))");
    expect(style).toContain("--normal-text: hsl(var(--ui-text-primary))");
    expect(style).toContain("--border-radius: var(--radius-lg)");
  });

  it("styles a success toast with a status icon, not with rich colours", async () => {
    render(<Toaster />);
    toast.success("Ready");
    await waitFor(() => expect(screen.getByText("Ready")).toBeInTheDocument());
    const item = document.querySelector(
      "[data-sonner-toast][data-type='success']",
    );
    expect(item?.getAttribute("class")).toContain("border-success/30");
    expect(item?.querySelector("[data-icon] svg")).not.toBeNull();
  });

  it("gives warning and error their own border tokens", async () => {
    render(<Toaster />);
    toast.warning("Needs Attention");
    toast.error("Action Required");
    await waitFor(() =>
      expect(screen.getByText("Action Required")).toBeInTheDocument(),
    );
    expect(
      document
        .querySelector("[data-sonner-toast][data-type='warning']")
        ?.getAttribute("class"),
    ).toContain("border-warning/30");
    expect(
      document
        .querySelector("[data-sonner-toast][data-type='error']")
        ?.getAttribute("class"),
    ).toContain("border-danger/30");
  });
});
