import { Command } from "cmdk";
import { Check, CornerDownLeft, Settings2 } from "lucide-react";
import type { ToolId } from "@/entities/tool";
import { cn } from "@/shared/ui/cn";
import { ServiceMark } from "@/shared/ui/ServiceArtwork";
import { ToolGlyph } from "@/shared/ui/ToolArtwork";
import type { ModelMenuEndpoint, ModelMenuItem } from "./homeModelMenu";

const ROW_CLASS = cn(
  "mx-1 flex min-w-0 cursor-default select-none items-center gap-2 rounded-md px-2 py-1.5 text-caption text-content outline-none",
  "data-[selected=true]:bg-layer-2",
);

/** The endpoint's logo or initials; the tool's own mark for its sign-in. */
export function EndpointMark({
  endpoint,
  tool,
}: {
  endpoint: ModelMenuEndpoint;
  tool: ToolId;
}) {
  return endpoint.signIn ? (
    <ToolGlyph toolId={tool} className="h-4 w-4 shrink-0" />
  ) : (
    <ServiceMark provider={endpoint.provider} name={endpoint.name} />
  );
}

export interface HomeModelRowsProps {
  endpoint: ModelMenuEndpoint;
  items: readonly ModelMenuItem[];
  loadingLabel: string;
  onChoose: (model: string | null) => void;
}

/** The endpoint's name over its models; the model in use is checked. */
export function HomeModelRows({
  endpoint,
  items,
  loadingLabel,
  onChoose,
}: HomeModelRowsProps) {
  return (
    <Command.Group
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
      {items.map((item) => (
        <Command.Item
          key={item.id}
          value={item.id}
          aria-current={item.checked ? "true" : undefined}
          onSelect={() => onChoose(item.model)}
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
          <Check
            className={cn(
              "h-3.5 w-3.5 shrink-0 text-brand",
              !item.checked && "invisible",
            )}
            aria-hidden="true"
          />
        </Command.Item>
      ))}
      {endpoint.loading && endpoint.models.length === 0 ? (
        <p className="px-3 py-1.5 text-caption text-content-muted">
          {loadingLabel}
        </p>
      ) : null}
    </Command.Group>
  );
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
