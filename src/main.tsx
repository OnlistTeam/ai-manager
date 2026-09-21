import React from "react";
import ReactDOM from "react-dom/client";
import { AppRoot } from "./app/index";
// tokens.css must load before index.css: it defines every colour variable the
// Tailwind base layer consumes.
import "./shared/styles/tokens.css";
import "./index.css";
import i18n, { i18nReady } from "./i18n";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "@/lib/query/queryClient";
import { native, onConfigLoadError, type InitErrorPayload } from "@/native";
import { FrontendErrorBoundary } from "./components/FrontendErrorBoundary";
import {
  installGlobalErrorHandlers,
  reportFrontendError,
} from "./lib/frontendLogger";
import { initializeWindowActivity } from "@/lib/windowActivity";
import { warmSessionEnvironment } from "@/app/warmSessionEnvironment";

installGlobalErrorHandlers();

// Add a body class based on platform, for platform-specific styling
try {
  const ua = navigator.userAgent || "";
  const plat = (navigator.platform || "").toLowerCase();
  const isMac = /mac/i.test(ua) || plat.includes("mac");
  if (isMac) {
    document.body.classList.add("is-mac");
  }
} catch {
  // Ignore platform-detection failures
}

/**
 * Handle a config load failure: show an error message and force the app to exit.
 * There is no "cancel" option for the user because the app cannot run with a broken config.
 */
async function handleConfigLoadError(
  payload: InitErrorPayload | null,
): Promise<void> {
  const [{ message }, { exit }] = await Promise.all([
    import("@tauri-apps/plugin-dialog"),
    import("@tauri-apps/plugin-process"),
  ]);
  const path =
    payload?.path ??
    i18n.t("errors.configFallbackPath", {
      defaultValue: "AI Manager application data/config.json",
    });
  const detail = payload?.error ?? "Unknown error";

  await message(
    i18n.t("errors.configLoadFailedMessage", {
      path,
      detail,
      defaultValue:
        "Unable to read configuration file:\n{{path}}\n\nError details:\n{{detail}}\n\nPlease check if the JSON is valid, or restore from a backup file (e.g., config.json.bak) in the same directory.\n\nThe app will exit so you can fix this.",
    }),
    {
      title: i18n.t("errors.configLoadFailedTitle", {
        defaultValue: "Configuration Load Failed",
      }),
      kind: "error",
    },
  );

  await exit(1);
}

async function bootstrap() {
  await i18nReady;

  // Listen for the backend's config-load-error event: only warn the user and
  // force an exit, never touch any config file.
  try {
    void onConfigLoadError((payload) => void handleConfigLoadError(payload));
  } catch (e) {
    // Ignore event-subscription failures (e.g. outside a Tauri environment)
    reportFrontendError("config_load_error_listener", e);
  }

  if (
    import.meta.env.DEV &&
    new URLSearchParams(window.location.search).get("gallery") ===
      "provider-presets"
  ) {
    const { ProviderPresetGallery } = await import(
      "./features/provider-management/__gallery__/ProviderPresetGallery"
    );
    ReactDOM.createRoot(document.getElementById("root")!).render(
      <React.StrictMode>
        <ProviderPresetGallery />
      </React.StrictMode>,
    );
    return;
  }

  // Dev-only design system gallery: `pnpm dev:renderer` then open
  // http://localhost:3000/?gallery. The dynamic import keeps the whole
  // gallery out of production bundles.
  if (
    import.meta.env.DEV &&
    new URLSearchParams(window.location.search).has("gallery")
  ) {
    const { DesignGallery } = await import(
      "./shared/ui/__gallery__/DesignGallery"
    );
    ReactDOM.createRoot(document.getElementById("root")!).render(
      <React.StrictMode>
        <DesignGallery />
      </React.StrictMode>,
    );
    return;
  }

  // Proactively query the backend for an early-startup init error, ahead of
  // any event race.
  try {
    const initError = await native.system.initError();
    if (initError && initError.kind === "db_version_too_new") {
      const [{ DatabaseUpgrade }, { Toaster }] = await Promise.all([
        import("./components/DatabaseUpgrade"),
        import("@/components/ui/sonner"),
      ]);
      // Database version is too new: render the in-app "upgrade app" recovery
      // screen instead of entering the normal app.
      ReactDOM.createRoot(document.getElementById("root")!).render(
        <React.StrictMode>
          <FrontendErrorBoundary>
            <DatabaseUpgrade payload={initError} />
            <Toaster />
          </FrontendErrorBoundary>
        </React.StrictMode>,
      );
      return;
    }
    if (initError && (initError.path || initError.error)) {
      await handleConfigLoadError(initError);
      // Unreachable: exit(1) above terminates the process first.
      return;
    }
  } catch (e) {
    // Ignore fetch failures and continue rendering
    reportFrontendError("get_init_error", e);
  }

  initializeWindowActivity();
  // One appearance, told to the native window once. Failures are ignored:
  // outside Tauri there is no native chrome to theme.
  void native.system.setWindowTheme("dark").catch((error: unknown) => {
    console.debug("Failed to set native window theme:", error);
  });
  // Do not await: the first screen paints immediately while the shared local
  // inventories warm once in the Query cache for the whole process.
  void warmSessionEnvironment(queryClient);

  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <FrontendErrorBoundary>
        <QueryClientProvider client={queryClient}>
          <AppRoot />
        </QueryClientProvider>
      </FrontendErrorBoundary>
    </React.StrictMode>,
  );
}

void bootstrap();
