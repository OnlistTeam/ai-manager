import type { TFunction } from "i18next";
import type { DiscoverInput, DiscoverMcpServer } from "@/entities/discover";

/** 3,556,437 as "3.6M" in the reader's language. */
export function compactCount(value: number, locale: string): string {
  try {
    return new Intl.NumberFormat(locale, {
      notation: "compact",
      maximumFractionDigits: 1,
    }).format(value);
  } catch {
    return String(value);
  }
}

/** How a server runs, as its badge says it. */
export function runsLabel(t: TFunction, server: DiscoverMcpServer): string {
  if (server.transport !== "stdio") {
    return t(server.signIn ? "discover.runs.signIn" : "discover.runs.remote");
  }
  return server.runs
    ? t(`discover.runs.${server.runs}`)
    : t("discover.runs.local");
}

/** A featured input is named in the reader's language; a registry one by its own name. */
export function inputLabel(t: TFunction, input: DiscoverInput): string {
  return input.kind ? t(`discover.input.${input.kind}`) : input.label;
}

/** A featured server's one-liner is product copy; a registry one is shown as returned. */
export function serverDescription(
  t: TFunction,
  server: DiscoverMcpServer,
): string | null {
  if (server.featured) {
    return (
      t(`discover.mcp.featured.${server.id}`, { defaultValue: "" }) || null
    );
  }
  return server.description;
}

export function requiredInputs(server: DiscoverMcpServer): DiscoverInput[] {
  return server.inputs.filter((input) => input.required);
}
