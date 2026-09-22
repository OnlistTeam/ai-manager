import type { TFunction } from "i18next";
import type {
  EffectiveConnectionSource,
  EffectiveCredential,
} from "@/entities/provider";

/** The full "where it comes from" sentence, shared wording between the top card and the override badge. */
export function describeSource(
  source: EffectiveConnectionSource,
  t: TFunction,
): string {
  switch (source.kind) {
    case "liveConfig":
      return t("services.effective.source.liveConfig", { path: source.path });
    case "shellFile":
      return t("services.effective.source.shellFile", {
        variable: source.variable,
        path: source.path,
      });
    case "environment":
      return t("services.effective.source.environment", {
        variable: source.variable,
      });
    case "toolDefault":
      return t("services.effective.source.toolDefault");
  }
}

/**
 * Whether two values come from the very same place.
 *
 * The address and the key usually do — one config file, or one profile line —
 * and repeating the identical "From …" sentence under each of them is noise
 * rather than information.
 */
export function sameSource(
  first: EffectiveConnectionSource,
  second: EffectiveConnectionSource,
): boolean {
  if (first.kind !== second.kind) return false;
  switch (first.kind) {
    case "liveConfig":
      return first.path === (second as typeof first).path;
    case "environment":
      return first.variable === (second as typeof first).variable;
    case "shellFile":
      return (
        first.variable === (second as typeof first).variable &&
        first.path === (second as typeof first).path
      );
    case "toolDefault":
      return true;
  }
}

/** The short name that fits in a badge: variable name or file path. */
export function sourceLabel(source: EffectiveConnectionSource): string {
  switch (source.kind) {
    case "shellFile":
    case "environment":
      return source.variable;
    case "liveConfig":
      return source.path;
    case "toolDefault":
      return "";
  }
}

/** Full wording for the credential state; the `switch` has no `default`, so a new state fails to compile. */
export function credentialCopy(
  credential: EffectiveCredential,
  toolName: string,
  t: TFunction,
): string {
  switch (credential) {
    case "configured":
      return t("services.effective.credential.configured");
    case "toolLogin":
      return t("services.effective.credential.toolLogin", { tool: toolName });
    case "missing":
      return t("services.effective.credential.missing");
    case "unknown":
      return t("services.effective.credential.unknown");
  }
}
