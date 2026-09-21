import { useState } from "react";
import {
  Boxes,
  HeartPulse,
  Home,
  Package,
  Puzzle,
  RadioTower,
  Settings,
  SlidersHorizontal,
  Wrench,
} from "lucide-react";
import type { DesktopApp } from "@/native/schemas/desktopApp";
import type { Tool, ToolCapabilities } from "@/native/schemas/tool";
import { DesktopAppCard } from "../../DesktopAppCard";
import { ProgressTask } from "../../ProgressTask";
import { ServiceCard } from "../../ServiceCard";
import { Sidebar, type SidebarItem } from "../../Sidebar";
import { SpatialPageHeader } from "../../SpatialPageHeader";
import {
  SpatialScene,
  type SpatialSceneModel,
  type SpatialSceneTone,
} from "../../SpatialScene";
import { ToolCard } from "../../ToolCard";

const CAPABILITIES: ToolCapabilities = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: true,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: true,
  canLaunch: true,
};

const TOOLS: Tool[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "ds.tool.installed",
    status: "installed",
    version: "2.3.1",
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: null,
  },
  {
    id: "codex",
    name: "Codex",
    descriptionKey: "ds.tool.updateAvailable",
    status: "updateAvailable",
    version: "1.4.0",
    latestVersion: "1.5.0",
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: null,
  },
  {
    id: "opencode",
    name: "OpenCode",
    descriptionKey: "ds.tool.notInstalled",
    status: "notInstalled",
    version: null,
    latestVersion: null,
    capabilities: CAPABILITIES,
    sessionsInsideSettings: false,
    environment: null,
  },
  {
    id: "gemini-cli",
    name: "Gemini CLI",
    descriptionKey: "ds.tool.broken",
    status: "broken",
    version: "0.9.0",
    latestVersion: null,
    capabilities: { ...CAPABILITIES, canRepair: false },
    sessionsInsideSettings: false,
    environment: null,
  },
];

const DESKTOP_APPS: DesktopApp[] = [
  {
    id: "codex-app",
    name: "ChatGPT / Codex",
    status: "installed",
    version: "26.527.60818",
    latestVersion: null,
    relatedTool: "codex",
    configurationRelationship: "sharedConfiguration",
    canLaunch: true,
    environment: "macos",
    installerHandoff: "directOfficialPackage",
    uninstallHandoff: "revealApplication",
    updatesManagedByVendor: true,
    canRollback: false,
    canManageMcp: false,
  },
  {
    id: "claude-desktop",
    name: "Claude Desktop",
    status: "notInstalled",
    version: null,
    latestVersion: null,
    relatedTool: "claude-code",
    configurationRelationship: "separateConfiguration",
    canLaunch: false,
    environment: "macos",
    installerHandoff: "directOfficialPackage",
    uninstallHandoff: "revealApplication",
    updatesManagedByVendor: true,
    canRollback: false,
    canManageMcp: false,
  },
];

const NAV: SidebarItem[] = [
  { id: "home", label: "Home", icon: Home },
  { id: "tools", label: "AI Tools", icon: Wrench, badge: 2 },
  { id: "services", label: "AI Services", icon: Package },
  { id: "extensions", label: "Extensions", icon: Puzzle },
];

const NAV_FOOTER: SidebarItem[] = [
  { id: "settings", label: "Settings", icon: Settings },
];

const SPATIAL_MODELS: readonly {
  model: SpatialSceneModel;
  label: string;
  icon: typeof Boxes;
  tone: SpatialSceneTone;
}[] = [
  {
    model: "environment",
    label: "Environment",
    icon: HeartPulse,
    tone: "success",
  },
  { model: "tools", label: "AI Tools", icon: Boxes, tone: "brand" },
  {
    model: "services",
    label: "AI Services",
    icon: RadioTower,
    tone: "success",
  },
  { model: "extensions", label: "Extensions", icon: Puzzle, tone: "warning" },
  {
    model: "settings",
    label: "Settings",
    icon: SlidersHorizontal,
    tone: "neutral",
  },
];

export function ProductSection() {
  const [activeId, setActiveId] = useState("tools");
  const [collapsed, setCollapsed] = useState(false);
  return (
    <section className="flex flex-col gap-4">
      <h2 className="text-title text-content">Product</h2>

      <div id="spatial-stage-gallery" data-gallery-section="spatial-stage">
        <SpatialPageHeader
          title="AI Tools"
          description="Install, update and keep every coding agent ready from one calm local workspace."
          icon={Boxes}
          model="tools"
        />
        <div className="mt-4 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {SPATIAL_MODELS.map(({ model, label, icon, tone }) => (
            <div
              key={model}
              data-spatial-gallery-model={model}
              className="flex min-h-56 min-w-0 flex-col items-center justify-center overflow-hidden rounded-2xl border border-hairline bg-layer-1 px-3 pb-4 backdrop-blur-xl"
            >
              <SpatialScene
                model={model}
                icon={icon}
                tone={tone}
                size="compact"
              />
              <h3 className="-mt-3 text-body font-medium text-content">
                {label}
              </h3>
            </div>
          ))}
        </div>
      </div>

      <div
        id="desktop-apps-gallery"
        data-gallery-section="desktop-apps"
        className="grid gap-4 lg:grid-cols-2"
      >
        {DESKTOP_APPS.map((app) => (
          <DesktopAppCard
            key={app.id}
            app={app}
            busy={false}
            error={null}
            onLaunch={() => undefined}
            onOpenOfficialDownload={() => undefined}
            onOpenUninstall={() => undefined}
            onShowDownloadHelp={() => undefined}
          />
        ))}
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        {TOOLS.map((tool) => (
          <ToolCard key={tool.id} tool={tool} onAction={() => undefined} />
        ))}
      </div>

      <div className="grid gap-4 md:grid-cols-2">
        <ServiceCard
          name="Claude"
          connected
          active
          usedBy={["Claude Code", "OpenCode"]}
          onUse={() => undefined}
        />
        <ServiceCard
          name="OpenRouter"
          connected
          usedBy={[]}
          onUse={() => undefined}
        />
        <ServiceCard name="Custom" usedBy={[]} onConnect={() => undefined} />
        <ServiceCard
          name="OpenAI"
          connected
          usedBy={["Codex"]}
          busy
          onUse={() => undefined}
        />
      </div>

      <div className="rounded-lg border border-hairline bg-layer-1 px-4">
        <ProgressTask
          name="Updating OpenCode"
          progress={72}
          status="running"
          detail="Downloading"
          onCancel={() => undefined}
        />
        <ProgressTask name="Checking Codex" progress={0} status="queued" />
        <ProgressTask
          name="Installing Gemini CLI"
          progress={100}
          status="success"
        />
        <ProgressTask
          name="Repairing Claude Code"
          progress={40}
          status="failed"
        />
        <ProgressTask
          name="Removing OpenCode"
          progress={12}
          status="cancelled"
        />
      </div>

      <div className="flex h-80 gap-4 overflow-hidden rounded-lg border border-hairline">
        <Sidebar
          items={NAV}
          footerItems={NAV_FOOTER}
          activeId={activeId}
          onSelect={setActiveId}
          collapsed={collapsed}
          onToggleCollapse={() => setCollapsed((value) => !value)}
          logo={<span className="text-heading text-content">AI Manager</span>}
        />
        <div className="flex-1 p-4 text-body text-content-muted">
          Selected: {activeId}
        </div>
      </div>
    </section>
  );
}
