import React from "react";
import { RefreshCw, TriangleAlert } from "lucide-react";
import i18n from "@/i18n";
import { reportFrontendError } from "@/lib/frontendLogger";
import { Button } from "@/components/ui/button";

interface FrontendErrorBoundaryState {
  hasError: boolean;
}

export class FrontendErrorBoundary extends React.Component<
  React.PropsWithChildren,
  FrontendErrorBoundaryState
> {
  state: FrontendErrorBoundaryState = { hasError: false };

  static getDerivedStateFromError(): FrontendErrorBoundaryState {
    return { hasError: true };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo): void {
    reportFrontendError(
      "react.error_boundary",
      error,
      info.componentStack ?? undefined,
    );
  }

  render(): React.ReactNode {
    if (!this.state.hasError) {
      return this.props.children;
    }

    return (
      <main className="flex min-h-screen items-center justify-center bg-background p-6 text-foreground">
        <section
          role="alert"
          className="w-full max-w-md space-y-5 rounded-lg border border-border bg-card p-6 shadow-sm"
        >
          <div className="flex items-start gap-3">
            <TriangleAlert className="mt-0.5 size-5 shrink-0 text-destructive" />
            <div className="space-y-1.5">
              <h1 className="text-base font-semibold">
                {i18n.t("errors.frontendCrashTitle", {
                  defaultValue: "Something went wrong in the interface",
                })}
              </h1>
              <p className="text-sm leading-6 text-muted-foreground">
                {i18n.t("errors.frontendCrashMessage", {
                  defaultValue:
                    "An attempt was made to write the error to the application diagnostic log. Reload the interface, and attach the log when opening an issue if the problem continues.",
                })}
              </p>
            </div>
          </div>
          <Button className="w-full" onClick={() => window.location.reload()}>
            <RefreshCw className="mr-2 size-4" />
            {i18n.t("errors.reloadInterface", {
              defaultValue: "Reload interface",
            })}
          </Button>
        </section>
      </main>
    );
  }
}
