import { AlertCircle, CheckCircle2, LogIn } from "lucide-react";
import type { TFunction } from "i18next";
import type {
  EffectiveCredential,
  Provider,
  ToolLoginStatus,
} from "@/entities/provider";
import { toolLoginSummary } from "./toolLoginCopy";

export interface ProviderKeyPresentation {
  icon: typeof CheckCircle2;
  text: string;
  iconClassName: string;
  textClassName: string;
  /** The raw key; only present when a key is actually stored, and the card uses this to decide whether to show a copy button. */
  copyValue: string | null;
  /** The tool is signed out: the card offers to sign in right there (ADR-0061). */
  signIn?: boolean;
}

/** Icon/text/color for the card's "key row": has a key, the tool's own sign-in (and whether it is signed in), or no key configured yet. */
export function providerKeyPresentation(
  provider: Provider,
  toolName: string,
  t: TFunction,
  credential?: EffectiveCredential,
  login?: ToolLoginStatus,
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
  // An account signed in here is named by the endpoint itself (ADR-0061);
  // the tool's sign-in right now is some other account's until it is used.
  if (provider.accountBound) {
    return {
      icon: LogIn,
      text: t("services.login.accountEndpoint"),
      iconClassName: "text-brand",
      textClassName: "text-caption",
      copyValue: null,
    };
  }
  // The tool's own answer about its sign-in beats "uses the tool's sign-in"
  // (ADR-0060); unread or unknown falls through to the lines below.
  const loginSummary =
    provider.kind === "official" ? toolLoginSummary(login, t) : null;
  if (loginSummary !== null) {
    const signedOut = login?.state === "signedOut";
    return {
      icon: signedOut ? AlertCircle : LogIn,
      text: loginSummary,
      iconClassName: signedOut ? "text-warning" : "text-brand",
      textClassName: "text-caption",
      copyValue: null,
      signIn: signedOut,
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
