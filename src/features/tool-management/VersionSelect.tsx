import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, ChevronDown, ChevronUp, GitBranch } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import type {
  TaggedVersionOption,
  VersionOptionGroups,
} from "./versionOptions";

interface VersionSelectProps {
  id: string;
  value: string;
  disabled: boolean;
  groups: VersionOptionGroups;
  onValueChange: (value: string) => void;
}

interface VersionItemProps {
  version: string;
  label?: string;
}

function VersionItem({ version, label = version }: VersionItemProps) {
  return (
    <SelectPrimitive.Item
      value={version}
      className="relative flex min-h-9 cursor-default select-none items-center rounded-lg py-2 pl-9 pr-3 font-mono text-mono-sm text-content outline-none data-[highlighted]:bg-layer-2 data-[highlighted]:text-content data-[state=checked]:bg-brand/10"
    >
      <span className="absolute left-3 flex h-4 w-4 items-center justify-center text-brand">
        <SelectPrimitive.ItemIndicator>
          <Check className="h-4 w-4" aria-hidden="true" />
        </SelectPrimitive.ItemIndicator>
      </span>
      <SelectPrimitive.ItemText>{label}</SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  );
}

function VersionGroup({
  label,
  versions,
}: {
  label: string;
  versions: readonly string[];
}) {
  if (versions.length === 0) return null;
  return (
    <SelectPrimitive.Group>
      <SelectPrimitive.Label className="px-3 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-content-faint">
        {label}
      </SelectPrimitive.Label>
      {versions.map((version) => (
        <VersionItem key={version} version={version} />
      ))}
    </SelectPrimitive.Group>
  );
}

function TaggedVersionGroup({
  label,
  versions,
}: {
  label: string;
  versions: readonly TaggedVersionOption[];
}) {
  const { t } = useTranslation();
  if (versions.length === 0) return null;
  return (
    <SelectPrimitive.Group>
      <SelectPrimitive.Label className="px-3 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-content-faint">
        {label}
      </SelectPrimitive.Label>
      {versions.map(({ version, tags }) => (
        <VersionItem
          key={version}
          version={version}
          label={t("tools.version.taggedOption", {
            version,
            tags: tags.join(", "),
          })}
        />
      ))}
    </SelectPrimitive.Group>
  );
}

/** A bounded, keyboard-accessible version menu that stays inside the window. */
export function VersionSelect({
  id,
  value,
  disabled,
  groups,
  onValueChange,
}: VersionSelectProps) {
  const { t } = useTranslation();
  const defaultVersion = groups.defaultVersion;

  return (
    <SelectPrimitive.Root
      value={value}
      disabled={disabled}
      onValueChange={onValueChange}
    >
      <SelectPrimitive.Trigger
        id={id}
        className={cn(
          "relative flex h-11 w-full items-center rounded-xl border border-hairline bg-layer-1 pl-10 pr-10 text-left font-mono text-mono-sm text-content shadow-sm backdrop-blur-xl",
          "transition-[border-color,background-color,box-shadow] duration-fast ease-standard hover:border-hairline hover:bg-layer-2",
          "disabled:cursor-not-allowed disabled:opacity-60",
          FOCUS_RING,
          // This control sits in the modal's scroll viewport. Paint its focus
          // ring inward so neither horizontal edge can be clipped.
          "focus-visible:ring-inset focus-visible:ring-offset-0",
        )}
      >
        <GitBranch
          className="pointer-events-none absolute left-3.5 h-4 w-4 text-content-muted"
          aria-hidden="true"
        />
        <SelectPrimitive.Value />
        <SelectPrimitive.Icon asChild>
          <ChevronDown
            className="pointer-events-none absolute right-3.5 h-4 w-4 text-content-muted"
            aria-hidden="true"
          />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>

      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          sideOffset={6}
          collisionPadding={12}
          className="z-[80] max-h-[min(22rem,var(--radix-select-content-available-height))] w-[var(--radix-select-trigger-width)] overflow-hidden rounded-xl border border-hairline/80 bg-surface/98 text-content shadow-lg backdrop-blur-2xl animate-ds-modal-in"
        >
          <SelectPrimitive.ScrollUpButton className="flex h-7 cursor-default items-center justify-center border-b border-hairline bg-layer-1 text-content-muted">
            <ChevronUp className="h-4 w-4" aria-hidden="true" />
          </SelectPrimitive.ScrollUpButton>
          <SelectPrimitive.Viewport className="scrollbar-subtle max-h-[19rem] p-1.5">
            {defaultVersion ? (
              <SelectPrimitive.Group>
                <SelectPrimitive.Label className="px-3 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-[0.08em] text-success">
                  {t("tools.version.defaultGroup")}
                </SelectPrimitive.Label>
                <VersionItem
                  version={defaultVersion}
                  label={t("tools.version.defaultOption", {
                    version: defaultVersion,
                  })}
                />
              </SelectPrimitive.Group>
            ) : null}
            <TaggedVersionGroup
              label={t("tools.version.taggedGroup")}
              versions={groups.taggedPublished}
            />
            <VersionGroup
              label={t("tools.version.otherPublishedGroup")}
              versions={groups.otherPublished}
            />
          </SelectPrimitive.Viewport>
          <SelectPrimitive.ScrollDownButton className="flex h-7 cursor-default items-center justify-center border-t border-hairline bg-layer-1 text-content-muted">
            <ChevronDown className="h-4 w-4" aria-hidden="true" />
          </SelectPrimitive.ScrollDownButton>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}
