import { render, screen, within } from "@testing-library/react";
import { Activity, CheckCircle2 } from "lucide-react";
import { describe, expect, it } from "vitest";
import { MetricStrip } from "@/shared/ui/MetricStrip";

describe("MetricStrip", () => {
  it("keeps its title, facts, detail and action semantic", () => {
    render(
      <MetricStrip
        eyebrow="Local facts"
        title="Tool readiness"
        description="Derived from the current inventory."
        highlight="12 ready"
        detail={<span>Last checked just now</span>}
        footer={<button type="button">Open details</button>}
        metrics={[
          {
            id: "ready",
            label: "Ready",
            value: 12,
            icon: CheckCircle2,
            iconClassName: "text-success",
          },
          { id: "running", label: "Running", value: 3, icon: Activity },
        ]}
      />,
    );

    const region = screen.getByRole("region", { name: "Tool readiness" });
    expect(
      within(region).getByRole("heading", {
        level: 2,
        name: "Tool readiness",
      }),
    ).toBeInTheDocument();
    expect(within(region).getByText("12 ready")).toBeInTheDocument();
    expect(
      within(region).getByText("Last checked just now"),
    ).toBeInTheDocument();
    expect(
      within(region).getByRole("button", { name: "Open details" }),
    ).toBeEnabled();
    expect(region.querySelectorAll("dl > div")).toHaveLength(2);
    expect(
      Array.from(region.querySelectorAll("dd"), (item) => item.textContent),
    ).toEqual(["12", "3"]);
  });

  it("uses one shared surface instead of metric cards", () => {
    const { container } = render(
      <MetricStrip
        title="Usage"
        metrics={[
          { id: "a", label: "A", value: 1, icon: Activity },
          { id: "b", label: "B", value: 2, icon: Activity },
          { id: "c", label: "C", value: 3, icon: Activity },
          { id: "d", label: "D", value: 4, icon: Activity },
        ]}
      />,
    );

    expect(container.querySelectorAll(".metric-strip__surface")).toHaveLength(
      1,
    );
    expect(container.querySelectorAll(".metric-strip__metric")).toHaveLength(4);
    expect(container.querySelector("dl")).toHaveClass("grid-cols-2");
    expect(container.querySelector("dl")).not.toHaveClass("sm:grid-cols-4");
    for (const metric of container.querySelectorAll(".metric-strip__metric")) {
      expect(metric).not.toHaveClass("bg-surface", "shadow-sm");
    }
  });
});
