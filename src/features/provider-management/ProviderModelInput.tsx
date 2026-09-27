import * as Popover from "@radix-ui/react-popover";
import { Command } from "cmdk";
import { ChevronDown, Loader2, Search } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ModelCatalog } from "@/native";
import { Button } from "@/shared/ui/Button";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { Input } from "@/shared/ui/Input";
import { cn } from "@/shared/ui/cn";

export interface ModelCatalogState {
  catalog: ModelCatalog | undefined;
  loading: boolean;
  error: Error | null;
  /**
   * The key or address in the form differs from the saved one. The list can
   * only be read with the saved pair, so it would describe a service the user
   * is moving away from.
   */
  stale: boolean;
  /** Called when the list is opened; reading it is a request to the service. */
  onRequest: () => void;
  onRetry: () => void;
}

interface ProviderModelInputProps {
  id: string;
  label: string;
  value: string;
  disabled: boolean;
  invalid: boolean;
  catalog: ModelCatalogState;
  onChange: (value: string) => void;
}

/**
 * A model name the user can type, with the service's own list one click away.
 * The text box stays the source of truth: plenty of relays publish no
 * `/v1/models`, or publish one that leaves models out.
 */
export function ProviderModelInput({
  id,
  label,
  value,
  disabled,
  invalid,
  catalog,
  onChange,
}: ProviderModelInputProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const models = catalog.catalog?.models ?? [];

  const status = catalog.stale
    ? t("services.form.modelListStale")
    : catalog.loading
      ? null
      : catalog.error
        ? t("services.form.modelListFailed")
        : catalog.catalog?.rejection
          ? t("services.probe.modelRefused", {
              status: catalog.catalog.rejection.status,
            })
          : models.length === 0
            ? t("services.probe.modelManualHint")
            : null;

  return (
    <div className="flex min-w-0 flex-1 gap-2">
      <Input
        id={id}
        aria-label={label}
        value={value}
        maxLength={256}
        spellCheck={false}
        autoComplete="off"
        disabled={disabled}
        invalid={invalid}
        className="min-w-0 font-mono"
        onChange={(event) => onChange(event.target.value)}
      />
      {/* Modal for the same reason as the preset picker: it opens from a
          dialog whose scroll lock would otherwise swallow the wheel. */}
      <Popover.Root
        modal
        open={open}
        onOpenChange={(next) => {
          setOpen(next);
          if (next && !catalog.stale) catalog.onRequest();
        }}
      >
        <Popover.Trigger asChild>
          <Button
            variant="secondary"
            disabled={disabled}
            aria-label={t("services.form.modelPick")}
            title={t("services.form.modelPick")}
          >
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          </Button>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content
            align="end"
            sideOffset={6}
            collisionPadding={16}
            className={cn(
              "app-floating-menu z-[220] w-80 max-w-[calc(100vw-2rem)] overflow-hidden rounded-xl border outline-none",
              "flex max-h-[min(22rem,var(--radix-popover-content-available-height))] flex-col",
            )}
          >
            {catalog.loading ? (
              <p
                role="status"
                className="flex items-center gap-2 px-4 py-6 text-body text-content-muted"
              >
                <Loader2
                  className="h-4 w-4 shrink-0 motion-safe:animate-spin"
                  aria-hidden="true"
                />
                {t("services.probe.modelLoading")}
              </p>
            ) : status !== null ? (
              <div className="flex flex-col items-start gap-3 px-4 py-4">
                <p role="status" className="text-body text-content-muted">
                  {status}
                </p>
                {catalog.error && !catalog.stale ? (
                  <Button
                    variant="secondary"
                    size="sm"
                    onClick={catalog.onRetry}
                  >
                    {t("ds.action.retry")}
                  </Button>
                ) : null}
              </div>
            ) : (
              <Command
                label={t("services.form.modelPick")}
                loop
                className="flex min-h-0 flex-col"
              >
                <div className="flex shrink-0 items-center gap-2 border-b border-hairline px-3">
                  <Search
                    className="h-4 w-4 shrink-0 text-content-muted"
                    aria-hidden="true"
                  />
                  <Command.Input
                    // eslint-disable-next-line jsx-a11y/no-autofocus -- search field of a list that was just opened on purpose
                    autoFocus
                    placeholder={t("services.form.modelSearch")}
                    className="h-11 min-w-0 flex-1 bg-transparent text-body text-content outline-none placeholder:text-content-muted"
                  />
                </div>
                <Command.List className="scrollbar-always min-h-0 flex-1 overflow-y-auto overscroll-contain py-1">
                  <Command.Empty className="px-4 py-6 text-center text-body text-content-muted">
                    {t("services.form.modelNoMatch")}
                  </Command.Empty>
                  {models.map((model) => (
                    <Command.Item
                      key={model.id}
                      value={model.id}
                      onSelect={() => {
                        onChange(model.id);
                        setOpen(false);
                      }}
                      className={cn(
                        "mx-1 cursor-pointer break-all rounded-lg px-3 py-2 font-mono text-caption text-content data-[selected=true]:bg-brand/[0.1]",
                        model.id === value && "text-brand",
                        FOCUS_RING,
                      )}
                    >
                      {model.id}
                    </Command.Item>
                  ))}
                </Command.List>
                {catalog.catalog?.truncated ? (
                  <p className="shrink-0 border-t border-hairline px-4 py-2 text-caption text-content-muted">
                    {t("services.probe.modelTruncated", {
                      count: models.length,
                    })}
                  </p>
                ) : null}
              </Command>
            )}
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
    </div>
  );
}
