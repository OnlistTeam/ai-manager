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
  services: "services.tab.endpoints",
  routing: "nav.routing",
  usage: "nav.usage",
};

export interface ServicesPageProps {
  preferredToolId?: ToolId | null;
  preferredTab?: ServicesTab | null;
  /** Opens the privacy section of the settings page. */
  onOpenPrivacySettings?: () => void;
}

/** One page heading; endpoints, local routing and usage are its inner tabs. */
export function ServicesPage({
  preferredToolId = null,
  preferredTab = null,
  onOpenPrivacySettings,
}: ServicesPageProps) {
  const { t } = useTranslation();
  const { tab, toolIntent, view, setView, select } = useServicesTab(
    preferredTab,
    preferredToolId,
  );
  // The add page puts a breadcrumb where the tabs were, so the way back is the
  // one thing above the cards.
  const adding = tab === "services" && view === "add";

  return (
    <div className="flex min-w-0 flex-col gap-4">
      <SectionHeader
        as="h1"
        title={t("services.title")}
        description={t("services.description")}
      />
      {adding ? null : (
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
      )}
      <section
        id={PANEL_ID}
        role={adding ? undefined : "tabpanel"}
        aria-labelledby={adding ? undefined : scopeTabId(TAB_PREFIX, tab)}
        className="flex min-w-0 flex-col"
      >
        {tab === "services" ? (
          <ServicesEndpointsPanel
            preferredToolId={toolIntent}
            view={view}
            onViewChange={setView}
          />
        ) : tab === "routing" ? (
          <RoutingPage onOpenPrivacySettings={onOpenPrivacySettings} />
        ) : (
          <UsagePage />
        )}
      </section>
    </div>
  );
}
