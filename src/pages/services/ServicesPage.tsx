import { useTranslation } from "react-i18next";
import type { ToolId } from "@/entities/tool";
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
  services: "nav.services",
  routing: "nav.routing",
  usage: "nav.usage",
};

export interface ServicesPageProps {
  preferredToolId?: ToolId | null;
  preferredTab?: ServicesTab | null;
}

/** One page heading; endpoints, local routing and usage are its inner tabs. */
export function ServicesPage({
  preferredToolId = null,
  preferredTab = null,
}: ServicesPageProps) {
  const { t } = useTranslation();
  const { tab, toolIntent, select } = useServicesTab(
    preferredTab,
    preferredToolId,
  );

  return (
    <div className="flex min-w-0 flex-col gap-5">
      <SectionHeader
        as="h1"
        title={t("services.title")}
        description={t("services.description")}
      />
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
          <RoutingPage />
        ) : (
          <UsagePage />
        )}
      </section>
    </div>
  );
}
