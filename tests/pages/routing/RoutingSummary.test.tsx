import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { describe, expect, it, vi } from "vitest";
import type { RoutingOverview } from "@/entities/routing";
import { RoutingSummary } from "@/pages/routing/RoutingSummary";
import en from "@/i18n/locales/en.json";
import zh from "@/i18n/locales/zh.json";

function overview(running = false, takeoverEnabled = running): RoutingOverview {
  return {
    running,
    address: running ? "127.0.0.1" : null,
    port: running ? 15721 : null,
    activeConnections: 0,
    totalRequests: 0,
    successRequests: 0,
    failedRequests: 0,
    failoverCount: 0,
    targets: [
      {
        tool: "claude-code",
        takeoverEnabled,
        autoFailoverEnabled: false,
        currentProvider: null,
        queue: [],
        available: [],
      },
    ],
  };
}

describe.each(["en", "zh"] as const)("RoutingSummary (%s)", (locale) => {
  const copy = locale === "en" ? en : zh;
  async function setup() {
    i18n.addResourceBundle(
      locale,
      "translation",
      { routing: copy.routing },
      true,
      true,
    );
    await i18n.changeLanguage(locale);
  }

  it("shows only a short sentence when stopped with no traffic", async () => {
    await setup();
    render(
      <RoutingSummary overview={overview()} busy={false} onStop={vi.fn()} />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      copy.routing.summary.inactive,
    );
    expect(screen.queryByText(copy.routing.summary.stats)).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByText(copy.routing.hero.title)).toBeNull();
    expect(screen.queryByText(copy.routing.hero.description)).toBeNull();
  });

  it("keeps counters collapsed and the stop action accessible while running", async () => {
    await setup();
    const user = userEvent.setup();
    const onStop = vi.fn();
    const data = {
      ...overview(true),
      totalRequests: 123,
      successRequests: 120,
      failedRequests: 3,
      failoverCount: 2,
    };
    const view = render(
      <RoutingSummary overview={data} busy={false} onStop={onStop} />,
    );
    expect(screen.getByText("127.0.0.1:15721")).toBeInTheDocument();
    expect(screen.getByText("123")).not.toBeVisible();
    await user.click(screen.getByText(copy.routing.summary.stats));
    expect(screen.getByText("123")).toBeVisible();
    expect(screen.getByText(copy.routing.hero.metric.failovers)).toBeVisible();
    await user.click(
      screen.getByRole("button", { name: copy.routing.stop.action }),
    );
    expect(onStop).toHaveBeenCalledOnce();
    view.rerender(<RoutingSummary overview={data} busy onStop={onStop} />);
    expect(
      screen.getByRole("button", { name: copy.routing.stop.action }),
    ).toBeDisabled();
  });

  it("does not hide restoration when the gateway stopped but a tool is still taken over", async () => {
    await setup();
    render(
      <RoutingSummary
        overview={overview(false, true)}
        busy={false}
        onStop={vi.fn()}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      copy.routing.summary.needsRestore,
    );
    expect(
      screen.getByRole("button", { name: copy.routing.stop.action }),
    ).toBeEnabled();
    expect(screen.queryByText(copy.routing.summary.inactive)).toBeNull();
  });

  it("retains nonzero statistics after stopping", async () => {
    await setup();
    const user = userEvent.setup();
    render(
      <RoutingSummary
        overview={{ ...overview(), totalRequests: 7 }}
        busy={false}
        onStop={vi.fn()}
      />,
    );
    expect(screen.getByText("7")).not.toBeVisible();
    await user.click(screen.getByText(copy.routing.summary.stats));
    expect(screen.getByText("7")).toBeVisible();
  });
});
