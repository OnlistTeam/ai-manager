import { AlertCircle, CheckCircle2, LogIn } from "lucide-react";
import type { TFunction } from "i18next";
import type { EffectiveCredential, Provider } from "@/entities/provider";

export interface ProviderKeyPresentation {
  icon: typeof CheckCircle2;
  text: string;
  iconClassName: string;
  textClassName: string;
  /** The raw key; only present when a key is actually stored, and the card uses this to decide whether to show a copy button. */
  copyValue: string | null;
}

/** Icon/text/color for the card's "key row": one of three choices — has a key, official sign-in without a key, or no key configured yet. */
export function providerKeyPresentation(
  provider: Provider,
  toolName: string,
  t: TFunction,
  credential?: EffectiveCredential,
): ProviderKeyPresentation {
  // Saved values remain visible/copyable independently of runtime credential evidence.
  if (provider.apiKey) {
    return {
      icon: CheckCircle2,
      text: provider.apiKey,
      iconClassName: "text-success",
      textClassName: "font-mono text-mono-sm",
      copyValue: provider.apiKey,
    };
  }
  if (credential) {
    return {
      icon:
        credential === "configured"
          ? CheckCircle2
          : credential === "toolLogin"
            ? LogIn
            : AlertCircle,
      text: t(`services.effective.credential.${credential}`, {
        tool: toolName,
      }),
      iconClassName:
        credential === "configured" ? "text-success" : "text-content-muted",
      textClassName: "text-caption",
      copyValue: null,
    };
  }
  if (provider.kind === "official") {
    return {
      icon: LogIn,
      text: t("services.card.toolSignIn", { tool: toolName }),
      iconClassName: "text-brand",
      textClassName: "text-caption",
      copyValue: null,
    };
  }
  return {
    icon: AlertCircle,
    text: t("services.card.noKey"),
    iconClassName: "text-warning",
    textClassName: "text-caption",
    copyValue: null,
  };
}
