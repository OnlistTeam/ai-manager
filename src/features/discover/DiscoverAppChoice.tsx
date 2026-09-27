import { useTranslation } from "react-i18next";
import {
  ScopeToggleGroup,
  type ExtensionScopeOption,
} from "@/features/extension-management";

export interface DiscoverAppChoiceProps {
  itemName: string;
  /** Apps that can take this item. */
  targets: readonly ExtensionScopeOption[];
  /** Installed apps that cannot run it, named once below the switches. */
  unreachable: readonly string[];
  selected: ReadonlySet<string>;
  disabled: boolean;
  onChange: (next: ReadonlySet<string>) => void;
}

/**
 * Which apps get the new item, with the same icon switches the list rows
 * use. Every app that can take it starts switched on.
 */
export function DiscoverAppChoice({
  itemName,
  targets,
  unreachable,
  selected,
  disabled,
  onChange,
}: DiscoverAppChoiceProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col gap-1.5">
      <span className="text-caption text-content">
        {t("discover.apps.label")}
      </span>
      {targets.length > 0 ? (
        <ScopeToggleGroup
          itemName={itemName}
          targets={targets}
          isOn={(target) => selected.has(target.key)}
          disabled={disabled}
          onToggle={(target, enabled) => {
            const next = new Set(selected);
            if (enabled) next.add(target.key);
            else next.delete(target.key);
            onChange(next);
          }}
        />
      ) : null}
      {targets.length > 0 && selected.size === 0 ? (
        <p role="status" className="text-caption text-warning">
          {t("discover.apps.none")}
        </p>
      ) : null}
      {unreachable.length > 0 ? (
        <p className="text-caption text-content-muted">
          {t("discover.apps.unreachable", { tools: unreachable })}
        </p>
      ) : null}
    </div>
  );
}
