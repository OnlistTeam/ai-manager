import { Command } from "cmdk";
import { Check, CornerDownLeft, Settings2, Star } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/ui/cn";
import type { ModelMenuItem, ModelMenuSection } from "./homeModelMenu";
import type { ModelFavorites } from "./useModelFavorites";

const ROW_CLASS = cn(
  "group mx-1 flex min-w-0 cursor-default select-none items-center gap-2 rounded-md px-2 py-1.5 text-caption text-content outline-none",
  "data-[selected=true]:bg-layer-2",
);

export interface HomeModelRowsProps {
  sections: readonly ModelMenuSection[];
  favorites: ModelFavorites;
  loadingLabel: string;
  onChoose: (item: ModelMenuItem) => void;
}

/** Each endpoint's name over its models; the model in use is checked. */
export function HomeModelRows({
  sections,
  favorites,
  loadingLabel,
  onChoose,
}: HomeModelRowsProps) {
  const { t } = useTranslation();

  return sections.map(({ endpoint, items }) => (
    <Command.Group
      key={endpoint.key}
      heading={
        <span
          title={endpoint.title ?? undefined}
          className="flex min-w-0 gap-2"
        >
          <span className="truncate">{endpoint.name}</span>
          {endpoint.detail ? (
            <span className="shrink-0 font-normal">{endpoint.detail}</span>
          ) : null}
        </span>
      }
      className="[&_[cmdk-group-heading]]:px-3 [&_[cmdk-group-heading]]:pb-1 [&_[cmdk-group-heading]]:pt-2 [&_[cmdk-group-heading]]:text-caption [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-content-muted"
    >
      {items.map((item) => {
        const starred = favorites.isFavorite(item);
        return (
          <Command.Item
            key={item.id}
            value={item.id}
            aria-current={item.checked ? "true" : undefined}
            onSelect={() => onChoose(item)}
            className={ROW_CLASS}
          >
            <span className="min-w-0 truncate">{item.label}</span>
            {item.note ? (
              <span className="min-w-0 flex-1 truncate text-content-muted">
                {item.note}
              </span>
            ) : (
              <span className="flex-1" />
            )}
            {item.model !== null ? (
              <button
                type="button"
                tabIndex={-1}
                aria-label={t(
                  starred ? "home.model.unfavorite" : "home.model.favorite",
                  { model: item.model },
                )}
                aria-pressed={starred}
                onPointerDown={(event) => event.preventDefault()}
                onClick={(event) => {
                  event.stopPropagation();
                  favorites.toggle(item);
                }}
                className={cn(
                  "flex h-5 w-5 shrink-0 items-center justify-center rounded text-content-muted hover:text-content",
                  !starred &&
                    "invisible group-hover:visible group-data-[selected=true]:visible",
                )}
              >
                <Star
                  className={cn(
                    "h-3.5 w-3.5",
                    starred && "fill-current text-warning",
                  )}
                  aria-hidden="true"
                />
              </button>
            ) : null}
            <Check
              className={cn(
                "h-3.5 w-3.5 shrink-0 text-brand",
                !item.checked && "invisible",
              )}
              aria-hidden="true"
            />
          </Command.Item>
        );
      })}
      {endpoint.loading && items.length === 1 ? (
        <p className="px-3 py-1.5 text-caption text-content-muted">
          {loadingLabel}
        </p>
      ) : null}
    </Command.Group>
  ));
}

/** Exactly what was typed, used as the model name. */
export function HomeModelTyped({
  label,
  onSelect,
}: {
  label: string;
  onSelect: () => void;
}) {
  return (
    <Command.Item value="typed" onSelect={onSelect} className={ROW_CLASS}>
      <CornerDownLeft
        className="h-3.5 w-3.5 shrink-0 text-content-muted"
        aria-hidden="true"
      />
      <span className="min-w-0 flex-1 truncate">{label}</span>
    </Command.Item>
  );
}

/** The way to the API Endpoints page, under every list. */
export function HomeModelManage({
  label,
  onSelect,
}: {
  label: string;
  onSelect: () => void;
}) {
  return (
    <Command.Item
      value="manage"
      onSelect={onSelect}
      className={cn(ROW_CLASS, "text-content-muted")}
    >
      <Settings2 className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      <span className="min-w-0 flex-1 truncate">{label}</span>
    </Command.Item>
  );
}
