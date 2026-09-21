import type React from "react";
import { AlertCircle, AlertTriangle, CheckCircle2, Info } from "lucide-react";
import { Toaster as SonnerToaster } from "sonner";

export interface ToasterProps {
  theme?: "light" | "dark" | "system";
}

/**
 * Product-themed sonner host. `richColors` is deliberately off: the status
 * hue lives on the icon and the border, and the text stays on the primary
 * text colour (spec §98, see the contrast table in the Phase 2 plan).
 *
 * sonner injects its own stylesheet for `[data-sonner-toast][data-styled='true']`
 * (background/border/color/border-radius via `--normal-*` CSS variables) at a
 * specificity that beats single-class Tailwind utilities. We therefore theme
 * through those variables via an inline style on `[data-sonner-toaster]`,
 * which wins over both the library's light and dark selectors. The classNames
 * below stay as a secondary, non-load-bearing layer.
 */
export function Toaster({ theme = "system" }: ToasterProps) {
  return (
    <SonnerToaster
      position="top-center"
      theme={theme}
      style={
        {
          "--normal-bg": "hsl(var(--ui-surface))",
          "--normal-border": "hsl(var(--ui-border))",
          "--normal-text": "hsl(var(--ui-text-primary))",
          "--border-radius": "var(--radius-lg)",
        } as React.CSSProperties
      }
      icons={{
        success: (
          <CheckCircle2 className="h-4 w-4 text-success" aria-hidden="true" />
        ),
        warning: (
          <AlertTriangle className="h-4 w-4 text-warning" aria-hidden="true" />
        ),
        error: (
          <AlertCircle className="h-4 w-4 text-danger" aria-hidden="true" />
        ),
        info: (
          <Info className="h-4 w-4 text-content-muted" aria-hidden="true" />
        ),
      }}
      toastOptions={{
        duration: 2000,
        classNames: {
          toast:
            "group rounded-lg border border-hairline bg-layer-1 text-content shadow-lg",
          title: "text-body font-medium text-content",
          description: "text-caption text-content-muted",
          closeButton:
            "border-hairline bg-layer-1 text-content-muted hover:bg-layer-2",
          actionButton:
            "rounded-md bg-brand px-3 py-1 text-caption font-medium text-brand-foreground hover:bg-brand-hover",
          cancelButton:
            "rounded-md bg-layer-1 px-3 py-1 text-caption text-content",
          success: "border-success/30",
          warning: "border-warning/30",
          error: "border-danger/30",
        },
      }}
    />
  );
}
