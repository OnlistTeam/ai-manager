import { LoaderCircle, ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { SpatialScene } from "@/shared/ui/SpatialScene";
import appIcon from "../../src-tauri/icons/128x128.png";

/** Visible while the product settings baseline the shell depends on is still loading. */
export function StartupScreen() {
  const { t } = useTranslation();

  return (
    <div
      role="status"
      aria-label={t("nav.starting")}
      aria-atomic="true"
      data-app-startup=""
      data-route="home"
      className="app-window-canvas relative flex h-full max-h-full w-full items-center justify-center overflow-hidden px-6 text-content"
    >
      <div className="relative z-10 grid w-full max-w-[900px] items-center gap-4 sm:gap-8 md:grid-cols-[minmax(280px,0.86fr)_minmax(0,1.14fr)]">
        <div className="flex min-h-64 items-center justify-center">
          <SpatialScene
            glyph={<img src={appIcon} alt="" />}
            model="environment"
            tone="success"
            size="hero"
            loading
          />
        </div>

        <div className="min-w-0 text-center md:text-left">
          <p className="inline-flex items-center gap-2 rounded-full border border-hairline bg-layer-1 px-3 py-1.5 text-caption text-content-muted shadow-sm">
            <ShieldCheck className="h-4 w-4 text-success" aria-hidden="true" />
            {t("nav.localFirst")}
          </p>
          <h1 className="mt-5 text-[clamp(38px,7vw,64px)] font-semibold leading-none tracking-[-0.045em] text-content">
            {t("nav.appName")}
          </h1>
          <p className="mt-4 inline-flex items-center gap-2 text-body text-content-muted">
            <LoaderCircle
              className="h-4 w-4 motion-safe:animate-spin"
              aria-hidden="true"
            />
            {t("nav.starting")}
          </p>
        </div>
      </div>
    </div>
  );
}
