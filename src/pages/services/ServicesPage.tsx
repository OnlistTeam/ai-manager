import { ArrowRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ToolId } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { ScopeTabs, scopeTabId } from "@/shared/ui/ScopeTabs";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { RoutingPage } from "@/pages/routing/RoutingPage";
import { UsagePage } from "@/pages/usage/UsagePage";
import { ServicesEndpointsPanel } from "./ServicesEndpointsPanel";
import {
  SERVICES_TABS,
  useServicesTab,
  type ServicesTab,
} from "./useServicesTab";

const TAB_PREFIX = "services-page";
const PANEL_ID = "services-page-panel";

const TAB_LABEL_KEYS: Record<ServicesTab, string> = {
  services: "services.tab.endpoints",
  routing: "nav.routing",
  usage: "nav.usage",
};

export interface ServicesPageProps {
  preferredToolId?: ToolId | null;
  preferredTab?: ServicesTab | null;
  /** Opens Home, where each tool's endpoint is chosen. */
  onOpenHome?: () => void;
  /** Opens the privacy section of the settings page. */
  onOpenPrivacySettings?: () => void;
}

/**
 * One page heading; endpoints, local routing and usage are its inner tabs.
 * The endpoints tab adds, edits, checks, orders and removes endpoints; which
 * one each tool uses is chosen on Home.
 */
export function ServicesPage({
  preferredToolId = null,
  preferredTab = null,
  onOpenHome,
  onOpenPrivacySettings,
}: ServicesPageProps) {
  const { t } = useTranslation();
  const { tab, toolIntent, select } = useServicesTab(
    preferredTab,
    preferredToolId,
  );

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <div className="min-w-0">
        <SectionHeader as="h1" title={t("services.title")} />
        {/* Choosing is Home's job (ADR-0053); this page keeps what you have. */}
        <p className="mt-0.5 flex flex-wrap items-center gap-x-1 text-caption text-content-muted">
          {t("services.chooseOnHome")}
          {onOpenHome ? (
            <Button
              variant="ghost"
              size="xs"
              className="-my-1 h-6 px-1.5"
              onClick={onOpenHome}
            >
              {t("services.openHome")}
              <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
            </Button>
          ) : null}
        </p>
      </div>
      <ScopeTabs
        items={SERVICES_TABS.map((id) => ({
          id,
          label: t(TAB_LABEL_KEYS[id]),
        }))}
        active={tab}
        label={t("services.title")}
        idPrefix={TAB_PREFIX}
        panelId={PANEL_ID}
        onSelect={(id) => {
          const picked = SERVICES_TABS.find((candidate) => candidate === id);
          if (picked) select(picked);
        }}
      />
      <section
        id={PANEL_ID}
        role="tabpanel"
        aria-labelledby={scopeTabId(TAB_PREFIX, tab)}
        className="flex min-w-0 flex-col"
      >
        {tab === "services" ? (
          <ServicesEndpointsPanel preferredToolId={toolIntent} />
        ) : tab === "routing" ? (
          <RoutingPage onOpenPrivacySettings={onOpenPrivacySettings} />
        ) : (
          <UsagePage />
        )}
      </section>
    </div>
  );
}
