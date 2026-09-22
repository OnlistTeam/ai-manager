import { useEffect, useState, type CSSProperties } from "react";
import { useTranslation } from "react-i18next";
import { DRAG_REGION_ATTR, DRAG_REGION_STYLE } from "@/lib/platform";
import { native } from "@/native";

/**
 * Height of the self-drawn title bar, matching what Windows itself uses for a
 * compact caption so the buttons land where muscle memory expects them.
 */
export const WINDOW_CONTROLS_HEIGHT = 32; // px

/**
 * Width of the three-button cluster. The content below reserves this much on
 * its right so nothing is ever tucked underneath the close button.
 */
export const WINDOW_CONTROLS_WIDTH = 138; // px

/** One caption button: no label, no border, the whole slab is the hit target. */
function ControlButton({
  label,
  onClick,
  danger = false,
  children,
}: {
  label: string;
  onClick: () => void;
  danger?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`flex h-full w-[46px] items-center justify-center text-content-muted outline-none transition-colors focus-visible:bg-layer-2 focus-visible:text-content ${
        danger
          ? "hover:bg-danger hover:text-white"
          : "hover:bg-layer-2 hover:text-content"
      }`}
    >
      {children}
    </button>
  );
}

/**
 * The window's caption, drawn by the product rather than the system.
 *
 * macOS has always drawn its traffic lights inside the client area
 * (`titleBarStyle: "Overlay"`), so the window there is one continuous surface.
 * Windows kept its native title bar, which meant a flat system-coloured slab
 * sitting above a gradient — the one visible seam in the product. This closes
 * it (ADR-0044).
 *
 * Linux keeps its native decorations: drag regions are disabled there to work
 * around a Wayland move-drag defect, and an undecorated window with no way to
 * drag it is worse than a seam.
 */
export function WindowControls() {
  const { t } = useTranslation();
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let cancelled = false;
    const sync = () => {
      void native.windowControls
        .isMaximized()
        .then((value) => {
          if (!cancelled) setMaximized(value);
        })
        .catch(() => {
          // A window state this cannot read only costs the restore glyph.
        });
    };
    sync();
    // Resize is the only event that can change the maximized state, including
    // the Aero Snap gestures that never go through our own button.
    const unlisten = native.windowControls
      .onResized(sync)
      .catch(() => () => {});
    return () => {
      cancelled = true;
      void unlisten.then((stop) => stop());
    };
  }, []);

  return (
    <div
      data-window-titlebar=""
      className="absolute inset-x-0 top-0 z-40 flex justify-end"
      style={{ height: WINDOW_CONTROLS_HEIGHT }}
    >
      {/* Everything left of the buttons drags the window, and a double-click
          maximizes it, which is what the system bar did. */}
      <div
        data-window-drag-region=""
        aria-hidden="true"
        className="h-full flex-1"
        style={{ ...DRAG_REGION_STYLE } as CSSProperties}
        {...DRAG_REGION_ATTR}
      />
      <div
        data-tauri-no-drag=""
        className="flex h-full shrink-0"
        style={{ width: WINDOW_CONTROLS_WIDTH }}
      >
        <ControlButton
          label={t("nav.window.minimize")}
          onClick={() => void native.windowControls.minimize()}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path d="M0 5h10" stroke="currentColor" strokeWidth="1" />
          </svg>
        </ControlButton>
        <ControlButton
          label={maximized ? t("nav.window.restore") : t("nav.window.maximize")}
          onClick={() => void native.windowControls.toggleMaximize()}
        >
          {maximized ? (
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <path
                d="M2.5 0.5h7v7h-2M0.5 2.5h7v7h-7z"
                fill="none"
                stroke="currentColor"
                strokeWidth="1"
              />
            </svg>
          ) : (
            <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
              <rect
                x="0.5"
                y="0.5"
                width="9"
                height="9"
                fill="none"
                stroke="currentColor"
                strokeWidth="1"
              />
            </svg>
          )}
        </ControlButton>
        <ControlButton
          label={t("nav.window.close")}
          danger
          onClick={() => void native.windowControls.close()}
        >
          <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
            <path
              d="M0.5 0.5l9 9M9.5 0.5l-9 9"
              stroke="currentColor"
              strokeWidth="1"
            />
          </svg>
        </ControlButton>
      </div>
    </div>
  );
}
