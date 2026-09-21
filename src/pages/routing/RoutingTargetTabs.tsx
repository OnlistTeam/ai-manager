import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { RoutingTarget } from "@/entities/routing";
import { ScopeTabs, scopeTabId } from "@/shared/ui/ScopeTabs";

export function RoutingTargetTabs({
  targets,
  disabled,
  children,
}: {
  targets: RoutingTarget[];
  disabled: boolean;
  children: (target: RoutingTarget) => ReactNode;
}) {
  const { t } = useTranslation();
  const [selectedTool, setSelectedTool] = useState<string | null>(null);
  const target =
    targets.find((item) => item.tool === selectedTool) ?? targets[0];
  if (!target) return null;
  return (
    <>
      <ScopeTabs
        items={targets.map((item) => ({
          id: item.tool,
          label: t(`routing.tool.${item.tool}`),
        }))}
        active={target.tool}
        label={t("routing.toolScope")}
        idPrefix="routing-tool"
        panelId="routing-tool-panel"
        disabled={disabled}
        onSelect={setSelectedTool}
      />
      <section
        id="routing-tool-panel"
        role="tabpanel"
        aria-labelledby={scopeTabId("routing-tool", target.tool)}
      >
        {children(target)}
      </section>
    </>
  );
}
