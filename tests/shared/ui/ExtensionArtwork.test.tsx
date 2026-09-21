import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ExtensionArtwork } from "@/shared/ui/ExtensionArtwork";

describe("ExtensionArtwork", () => {
  it.each(["skill", "mcp", "prompt"] as const)(
    "renders a decorative %s identity without remote artwork",
    (kind) => {
      const { container } = render(<ExtensionArtwork kind={kind} />);
      const artwork = container.querySelector(
        `[data-extension-artwork="${kind}"]`,
      );
      expect(artwork).toBeInTheDocument();
      expect(artwork).toHaveAttribute("aria-hidden", "true");
      expect(screen.queryByRole("img")).toBeNull();
    },
  );
});
