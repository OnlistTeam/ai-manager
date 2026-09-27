import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { Provider } from "@/entities/provider";
import { ServiceArtwork, ServiceMark } from "@/shared/ui/ServiceArtwork";

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

describe("ServiceMark", () => {
  // ADR-0059: an address set outside the app is named by its host; the mark
  // takes the site's name, so `api.onlist.net` is not a different "AP" service.
  it.each([
    ["api.onlist.net", "ON"],
    ["api.onlist.net:8443", "ON"],
    ["www.example.co.uk", "EX"],
    ["openrouter.ai", "OP"],
    ["localhost:11434", "LO"],
    ["127.0.0.1:18765", "12"],
    ["Team Relay", "TR"],
  ])("marks %s as %s", (name, expected) => {
    const { container } = render(<ServiceMark provider={null} name={name} />);
    expect(
      container.querySelector('[data-service-artwork="generic"]'),
    ).toHaveTextContent(expected);
  });
});
