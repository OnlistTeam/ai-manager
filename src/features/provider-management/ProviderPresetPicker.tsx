import * as Popover from "@radix-ui/react-popover";
import { Command } from "cmdk";
import {
  BadgeCheck,
  Check,
  ChevronsUpDown,
  Gauge,
  Globe2,
  LoaderCircle,
  Search,
} from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  ProviderConnectionPreset,
  ProviderEndpointTestResult,
} from "@/entities/provider";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { cn } from "@/shared/ui/cn";

interface ProviderPresetPickerProps {
  id: string;
  presets: ProviderConnectionPreset[];
  value: string;
  disabled?: boolean;
  measurements: ProviderEndpointTestResult[];
  measured: boolean;
  measuring: boolean;
  measurementFailed: boolean;
  onMeasure: () => void;
  onChange: (presetId: string) => void;
}

export function sortPresetsByMeasurement(
  presets: ProviderConnectionPreset[],
  measurements: ProviderEndpointTestResult[],
): ProviderConnectionPreset[] {
  const resultById = new Map(
    measurements.map((measurement) => [measurement.candidateId, measurement]),
  );
  const originalIndex = new Map(
    presets.map((preset, index) => [preset.id, index]),
  );
  return [...presets].sort((left, right) => {
    const leftResult = resultById.get(left.id);
    const rightResult = resultById.get(right.id);
    const leftLatency =
      leftResult?.failure === null && leftResult.latencyMs !== null
        ? leftResult.latencyMs
        : Number.POSITIVE_INFINITY;
    const rightLatency =
      rightResult?.failure === null && rightResult.latencyMs !== null
        ? rightResult.latencyMs
        : Number.POSITIVE_INFINITY;
    return (
      leftLatency - rightLatency ||
      (originalIndex.get(left.id) ?? 0) - (originalIndex.get(right.id) ?? 0)
    );
  });
}

function PresetGroup({
  label,
  presets,
  value,
  measurements,
  onSelect,
}: {
  label: string;
  presets: ProviderConnectionPreset[];
  value: string;
  measurements: ReadonlyMap<string, ProviderEndpointTestResult>;
  onSelect: (presetId: string) => void;
}) {
  const { t } = useTranslation();
  if (presets.length === 0) return null;
  return (
    <Command.Group
      heading={label}
      className="p-1.5 text-content [&_[cmdk-group-heading]]:px-2.5 [&_[cmdk-group-heading]]:pb-1.5 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:text-caption [&_[cmdk-group-heading]]:font-semibold [&_[cmdk-group-heading]]:uppercase [&_[cmdk-group-heading]]:tracking-[0.12em] [&_[cmdk-group-heading]]:text-content-muted"
    >
      {presets.map((preset) => {
        const selected = preset.id === value;
        const measurement = measurements.get(preset.id);
        const Icon = preset.official ? BadgeCheck : Globe2;
        return (
          <Command.Item
            key={preset.id}
            value={preset.id}
            keywords={[preset.serviceName, preset.defaultModel]}
            onSelect={() => onSelect(preset.id)}
            className="flex cursor-default select-none items-center gap-2.5 rounded-lg px-2.5 py-2 text-body outline-none data-[selected=true]:bg-brand/[0.11] data-[selected=true]:text-content"
          >
            <span
              className={cn(
                "flex h-7 w-7 shrink-0 items-center justify-center rounded-lg border",
                preset.official
                  ? "border-brand/20 bg-brand/10 text-brand"
                  : "border-hairline bg-layer-1 text-content-muted",
              )}
            >
              <Icon className="h-3.5 w-3.5" aria-hidden="true" />
            </span>
            <span className="min-w-0 flex-1">
              <span className="block truncate font-medium">
                {preset.serviceName}
              </span>
              <span className="block truncate text-caption text-content-muted">
                {preset.defaultModel}
              </span>
            </span>
            {measurement ? (
              <span
                className={cn(
                  "shrink-0 rounded-full border px-2 py-0.5 text-caption tabular-nums",
                  measurement.failure === null
                    ? "border-brand/20 bg-brand/10 text-brand"
                    : "border-hairline bg-layer-1 text-content-muted",
                )}
              >
                {measurement.failure === null && measurement.latencyMs !== null
                  ? t("services.connect.responseTime", {
                      latency: measurement.latencyMs,
                    })
                  : t("services.connect.unreachable")}
              </span>
            ) : null}
            <Check
              className={cn(
                "h-4 w-4 shrink-0 text-brand",
                selected ? "opacity-100" : "opacity-0",
              )}
              aria-hidden="true"
            />
          </Command.Item>
        );
      })}
    </Command.Group>
  );
}

export function ProviderPresetPicker({
  id,
  presets,
  value,
  disabled = false,
  measurements,
  measured,
  measuring,
  measurementFailed,
  onMeasure,
  onChange,
}: ProviderPresetPickerProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const selected = presets.find((preset) => preset.id === value) ?? presets[0];
  const measurementById = useMemo(
    () =>
      new Map(
        measurements.map((measurement) => [
          measurement.candidateId,
          measurement,
        ]),
      ),
    [measurements],
  );
  const groups = useMemo(() => {
    if (measured) {
      return {
        measured: sortPresetsByMeasurement(presets, measurements),
        official: [],
        compatible: [],
      };
    }
    return {
      measured: [],
      official: presets.filter((preset) => preset.official),
      compatible: presets.filter((preset) => !preset.official),
    };
  }, [measured, measurements, presets]);
  const select = (presetId: string) => {
    onChange(presetId);
    setOpen(false);
  };

  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="text-caption text-content">
        {t("services.connect.preset")}
      </label>
      <Popover.Root open={open} onOpenChange={setOpen}>
        <Popover.Trigger asChild>
          <button
            id={id}
            type="button"
            // eslint-disable-next-line jsx-a11y/role-has-required-aria-props -- Radix Popover.Trigger injects aria-controls at runtime
            role="combobox"
            aria-expanded={open}
            disabled={disabled}
            className={cn(
              "flex min-h-10 w-full items-center justify-between gap-3 rounded-lg border border-hairline bg-layer-1 px-3 text-left text-body text-content shadow-sm transition-colors hover:border-brand/35 disabled:cursor-not-allowed disabled:opacity-50",
              FOCUS_RING,
            )}
          >
            <span className="min-w-0 flex-1 truncate font-medium">
              {selected?.serviceName}
            </span>
            <span className="flex shrink-0 items-center gap-2 text-caption text-content-muted">
              {selected?.official
                ? t("services.connect.official")
                : t("services.connect.compatible")}
              <ChevronsUpDown className="h-4 w-4" aria-hidden="true" />
            </span>
          </button>
        </Popover.Trigger>
        <Popover.Portal>
          <Popover.Content
            align="start"
            sideOffset={6}
            collisionPadding={16}
            // `bg-layer-1` is 8% white: a token for a panel nested inside an
            // opaque surface, not for a list floating over one, where it let
            // the dialog's own labels read straight through it.
            className="app-floating-menu z-[220] w-[var(--radix-popover-trigger-width)] max-w-[calc(100vw-2rem)] overflow-hidden rounded-xl border outline-none"
          >
            <Command label={t("services.connect.preset")} loop>
              <div className="flex items-center gap-2 border-b border-hairline px-3">
                <Search
                  className="h-4 w-4 shrink-0 text-content-muted"
                  aria-hidden="true"
                />
                <Command.Input
                  // eslint-disable-next-line jsx-a11y/no-autofocus -- combobox search field, focused only when the popover opens
                  autoFocus
                  placeholder={t("services.connect.searchPresets")}
                  className="h-11 min-w-0 flex-1 bg-transparent text-body text-content outline-none placeholder:text-content-muted"
                />
              </div>
              <div className="border-b border-hairline px-3 py-2.5">
                <button
                  type="button"
                  disabled={disabled || measuring}
                  onClick={onMeasure}
                  className={cn(
                    "flex min-h-9 w-full items-center justify-center gap-2 rounded-lg border border-hairline bg-layer-1 px-3 text-body font-medium text-content transition-colors hover:border-brand/35 hover:bg-brand/[0.07] disabled:cursor-not-allowed disabled:opacity-55",
                    FOCUS_RING,
                  )}
                >
                  {measuring ? (
                    <LoaderCircle
                      className="h-4 w-4 animate-spin"
                      aria-hidden="true"
                    />
                  ) : (
                    <Gauge className="h-4 w-4 text-brand" aria-hidden="true" />
                  )}
                  {t(
                    measuring
                      ? "services.connect.speedTesting"
                      : measured
                        ? "services.connect.speedTestAgain"
                        : "services.connect.speedTest",
                  )}
                </button>
                {measurementFailed ? (
                  <p className="mt-2 text-caption text-danger" role="alert">
                    {t("services.connect.speedTestFailed")}
                  </p>
                ) : measured ? (
                  <p
                    className="mt-2 text-caption text-content-muted"
                    role="status"
                  >
                    {t("services.connect.speedSorted")}
                  </p>
                ) : null}
              </div>
              <Command.List className="max-h-72 overflow-y-auto overscroll-contain py-1">
                <Command.Empty className="px-4 py-8 text-center text-body text-content-muted">
                  {t("services.connect.noPresetMatch")}
                </Command.Empty>
                <PresetGroup
                  label={t("services.connect.measuredServices")}
                  presets={groups.measured}
                  value={value}
                  measurements={measurementById}
                  onSelect={select}
                />
                <PresetGroup
                  label={t("services.connect.officialServices")}
                  presets={groups.official}
                  value={value}
                  measurements={measurementById}
                  onSelect={select}
                />
                <PresetGroup
                  label={t("services.connect.compatibleServices")}
                  presets={groups.compatible}
                  value={value}
                  measurements={measurementById}
                  onSelect={select}
                />
              </Command.List>
            </Command>
          </Popover.Content>
        </Popover.Portal>
      </Popover.Root>
      <p className="text-caption leading-5 text-content-muted">
        {t("services.connect.presetHint")}
      </p>
    </div>
  );
}
