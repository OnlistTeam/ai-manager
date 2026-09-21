import { Globe2, RadioTower, SquareTerminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/ui/cn";
import type { McpTransport } from "./mcpInstallForm";

const TRANSPORTS = [
  { id: "stdio", icon: SquareTerminal },
  { id: "http", icon: Globe2 },
  { id: "sse", icon: RadioTower },
] as const;

interface McpTransportPickerProps {
  value: McpTransport;
  disabled: boolean;
  onChange: (transport: McpTransport) => void;
}

export function McpTransportPicker({
  value,
  disabled,
  onChange,
}: McpTransportPickerProps) {
  const { t } = useTranslation();

  return (
    <fieldset>
      <legend className="text-caption text-content">
        {t("extensions.mcp.install.transport.label")}
      </legend>
      <div className="mt-1.5 grid gap-2 sm:grid-cols-3">
        {TRANSPORTS.map(({ id, icon: Icon }) => (
          <label key={id} className="min-w-0 cursor-pointer">
            <input
              type="radio"
              name="mcp-transport"
              value={id}
              checked={value === id}
              disabled={disabled}
              onChange={() => onChange(id)}
              className="peer sr-only"
            />
            <span
              className={cn(
                "flex h-full min-h-20 flex-col rounded-lg border border-hairline bg-layer-1 p-3 text-content-muted",
                "transition-colors duration-fast hover:border-content-muted hover:bg-layer-2",
                "peer-checked:border-brand/50 peer-checked:bg-brand/5 peer-checked:text-content",
                "peer-focus-visible:ring-2 peer-focus-visible:ring-brand peer-focus-visible:ring-offset-2 peer-focus-visible:ring-offset-surface",
                "peer-disabled:cursor-not-allowed peer-disabled:opacity-50",
              )}
            >
              <span className="flex items-center gap-2 text-caption font-medium">
                <Icon className="h-4 w-4 text-brand" aria-hidden="true" />
                {t(`extensions.mcp.install.transport.${id}.label`)}
              </span>
              <span className="mt-1 text-caption leading-5 text-content-muted">
                {t(`extensions.mcp.install.transport.${id}.description`)}
              </span>
            </span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}
