import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ProgressTask } from "@/shared/ui/ProgressTask";

describe("ProgressTask", () => {
  it("shows the name, the status badge and the percentage while running", () => {
    render(
      <ProgressTask name="Updating OpenCode" progress={72} status="running" />,
    );
    expect(screen.getByText("Updating OpenCode")).toBeInTheDocument();
    expect(screen.getByText("ds.task.running")).toBeInTheDocument();
    expect(screen.getByText("72%")).toBeInTheDocument();
    expect(screen.getByRole("progressbar")).toBeInTheDocument();
  });

  it("keeps a long user-owned task name readable beside its status", async () => {
    const name =
      "Add SharedTeamProductionFilesystemConnectionUsedByEveryWorkstationAnywhere MCP connection";
    const onCancel = vi.fn();
    render(
      <ProgressTask
        name={name}
        progress={45}
        status="running"
        progressLabel={name}
        onCancel={onCancel}
      />,
    );

    const taskName = screen.getByText(name);
    expect(taskName).toHaveClass("break-words", "flex-[1_1_12rem]");
    expect(taskName).not.toHaveClass("truncate");
    expect(taskName.parentElement).toHaveClass("flex-wrap", "items-start");
    expect(screen.getByText("45%").parentElement).toHaveClass("ml-auto");
    expect(screen.getByRole("progressbar", { name })).toBeInTheDocument();

    await userEvent.tab();
    expect(
      screen.getByRole("button", { name: "ds.action.cancel" }),
    ).toHaveFocus();
  });

  it("hides the bar once the task reached a terminal state", () => {
    const { rerender } = render(
      <ProgressTask name="Updating OpenCode" progress={100} status="success" />,
    );
    expect(screen.queryByRole("progressbar")).toBeNull();
    expect(screen.getByText("ds.task.success")).toBeInTheDocument();
    rerender(
      <ProgressTask name="Updating OpenCode" progress={40} status="failed" />,
    );
    expect(screen.getByText("ds.task.failed")).toBeInTheDocument();
  });

  it("offers cancel only while the task can still be stopped", async () => {
    const onCancel = vi.fn();
    const { rerender } = render(
      <ProgressTask
        name="Updating OpenCode"
        progress={10}
        status="running"
        onCancel={onCancel}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.cancel" }),
    );
    expect(onCancel).toHaveBeenCalledTimes(1);
    rerender(
      <ProgressTask
        name="Updating OpenCode"
        progress={100}
        status="success"
        onCancel={onCancel}
      />,
    );
    expect(
      screen.queryByRole("button", { name: "ds.action.cancel" }),
    ).toBeNull();
  });

  it("renders the optional human-readable detail line", () => {
    render(
      <ProgressTask
        name="Installing Claude Code"
        progress={20}
        status="running"
        detail="Downloading"
      />,
    );
    expect(screen.getByText("Downloading").className).toContain(
      "text-content-muted",
    );
  });

  it("accepts a unique progress label when several tasks share a page", () => {
    render(
      <ProgressTask
        name="Updating OpenCode"
        progress={72}
        status="running"
        progressLabel="Update OpenCode"
      />,
    );

    expect(
      screen.getByRole("progressbar", { name: "Update OpenCode" }),
    ).toHaveAttribute("aria-valuenow", "72");
  });
});
