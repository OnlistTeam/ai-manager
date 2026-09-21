import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Provider } from "@/entities/provider";
import { ServiceArtwork } from "@/shared/ui/ServiceArtwork";

function provider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: "relay",
    tool: "claude-code",
    name: "My Relay",
    kind: "custom",
    active: false,
    baseUrl: "https://relay.example.com",
    apiKey: null,
    websiteUrl: null,
    testable: true,
    canRemove: true,
    ...overrides,
  };
}

describe("ServiceArtwork", () => {
  it("uses the owning tool's first-party artwork for an official service", () => {
    const { container } = render(
      <ServiceArtwork
        provider={provider({ kind: "official", name: "Claude Official" })}
      />,
    );
    expect(
      container.querySelector('[data-service-artwork="anthropic"]'),
    ).toBeInTheDocument();
  });

  it("recognizes a known service from its exact domain", () => {
    const { container } = render(
      <ServiceArtwork
        provider={provider({
          name: "Personal route",
          baseUrl: "https://openrouter.ai/api/v1",
        })}
      />,
    );
    expect(
      container.querySelector('[data-service-artwork="openrouter"]'),
    ).toBeInTheDocument();
  });

  it("falls back to calm initials without claiming an unknown brand", () => {
    const { container } = render(
      <ServiceArtwork provider={provider({ name: "Team Relay" })} />,
    );
    const artwork = container.querySelector('[data-service-artwork="generic"]');
    expect(artwork).toHaveTextContent("TR");
    expect(artwork).toHaveAttribute("aria-hidden", "true");
  });
});
