import * as React from "react";
import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "./cn";
import { FOCUS_RING } from "./focusRing";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "./Tooltip";

export interface SidebarItem {
  id: string;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
  badge?: number;
}

export interface SidebarProps {
  /** Navigation is data, not hard-coded routes: the shell owns the list. */
  items: SidebarItem[];
  /** Destinations pinned to the bottom of the rail, e.g. Settings. */
  footerItems?: SidebarItem[];
  activeId: string;
  onSelect: (id: string) => void;
  collapsed?: boolean;
  onToggleCollapse?: () => void;
  logo?: React.ReactNode;
  className?: string;
}

interface CompactSidebarTooltipProps {
  children: React.ReactElement;
  label: string;
  visible: boolean;
}

function CompactSidebarTooltip({
  children,
  label,
  visible,
}: CompactSidebarTooltipProps) {
  const [open, setOpen] = React.useState(false);

  React.useEffect(() => {
    if (!visible) setOpen(false);
  }, [visible]);

  return (
    <Tooltip
      open={visible && open}
      onOpenChange={(nextOpen) => {
        if (visible) setOpen(nextOpen);
      }}
    >
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      {visible ? (
        <TooltipContent side="right" sideOffset={12} collisionPadding={12}>
          {label}
        </TooltipContent>
      ) : null}
    </Tooltip>
  );
}

interface SidebarNavButtonProps {
  item: SidebarItem;
  active: boolean;
  collapsed: boolean;
  onSelect: (id: string) => void;
}

function SidebarNavButton({
  item,
  active,
  collapsed,
  onSelect,
}: SidebarNavButtonProps) {
  const Icon = item.icon;
  return (
    <CompactSidebarTooltip label={item.label} visible={collapsed}>
      <button
        type="button"
        onClick={() => onSelect(item.id)}
        aria-current={active ? "page" : undefined}
        data-active={active || undefined}
        className={cn(
          // Geometry and the two selection states live in index.css
          // (.sidebar-nav-item); the rail is one of the few places where the
          // treatment depends on the active route's palette.
          "sidebar-nav-item relative flex h-[52px] items-center gap-3 px-3 text-body",
          collapsed && "justify-center",
          active ? "text-content" : "text-content/85 hover:text-content",
          FOCUS_RING,
        )}
      >
        <span className="sidebar-nav-icon flex h-8 w-8 shrink-0 items-center justify-center rounded-lg">
          <Icon className="h-[18px] w-[18px] shrink-0" aria-hidden="true" />
        </span>
        <span className={cn("truncate", collapsed && "sr-only")}>
          {item.label}
        </span>
        {item.badge !== undefined && !collapsed ? (
          <span className="ml-auto text-caption text-content/75">
            {item.badge}
          </span>
        ) : null}
      </button>
    </CompactSidebarTooltip>
  );
}

export function Sidebar({
  items,
  footerItems = [],
  activeId,
  onSelect,
  collapsed = false,
  onToggleCollapse,
  logo,
  className,
}: SidebarProps) {
  const { t } = useTranslation();
  const toggleLabel = collapsed
    ? t("ds.sidebar.expand")
    : t("ds.sidebar.collapse");

  const renderItems = (list: SidebarItem[]) =>
    list.map((item) => (
      <SidebarNavButton
        key={item.id}
        item={item}
        active={item.id === activeId}
        collapsed={collapsed}
        onSelect={onSelect}
      />
    ));

  return (
    <TooltipProvider
      delayDuration={180}
      skipDelayDuration={80}
      disableHoverableContent
    >
      <div
        className={cn(
          // Spec §52 fixes the two widths.
          collapsed ? "w-[72px]" : "w-[220px]",
          "flex h-full shrink-0 select-none flex-col gap-6 bg-transparent p-3.5",
          "transition-[width] duration-base ease-standard",
          className,
        )}
      >
        {logo ? (
          <div
            className={cn(
              "flex h-10 items-center",
              collapsed ? "justify-center" : "px-2.5",
            )}
          >
            {logo}
          </div>
        ) : null}

        <nav
          aria-label={t("ds.sidebar.primary")}
          className="flex min-h-0 flex-1 flex-col gap-3"
        >
          <div className="flex min-h-0 flex-col gap-1 overflow-y-auto">
            {renderItems(items)}
          </div>
          {footerItems.length > 0 ? (
            <div
              data-sidebar-footer=""
              className="mt-auto flex shrink-0 flex-col gap-1"
            >
              {renderItems(footerItems)}
            </div>
          ) : null}
        </nav>

        {onToggleCollapse ? (
          <CompactSidebarTooltip label={toggleLabel} visible={collapsed}>
            <button
              type="button"
              onClick={onToggleCollapse}
              className={cn(
                "flex min-h-10 w-full items-center gap-3 rounded-sm px-2.5 py-2 text-body text-content/75",
                "transition-colors duration-fast ease-standard hover:bg-layer-2 hover:text-content",
                collapsed && "justify-center",
                FOCUS_RING,
              )}
            >
              {collapsed ? (
                <PanelLeftOpen
                  className="h-4 w-4 shrink-0"
                  aria-hidden="true"
                />
              ) : (
                <PanelLeftClose
                  className="h-4 w-4 shrink-0"
                  aria-hidden="true"
                />
              )}
              <span
                className={cn(
                  "min-w-0 flex-1 whitespace-normal break-words text-left text-caption leading-4",
                  collapsed && "sr-only",
                )}
              >
                {toggleLabel}
              </span>
            </button>
          </CompactSidebarTooltip>
        ) : null}
      </div>
    </TooltipProvider>
  );
}
