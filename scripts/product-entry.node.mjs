import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function source(path) {
  return readFile(new URL(path, root), "utf8");
}

async function sourceFiles(path) {
  try {
    const entries = await readdir(new URL(path, root), { recursive: true });
    return entries
      .map((entry) => entry.replaceAll("\\", "/"))
      .filter((entry) => /\.(?:ts|tsx)$/.test(entry))
      .sort();
  } catch (error) {
    if (error?.code === "ENOENT") return [];
    throw error;
  }
}

test("the production entry mounts only the product shell", async () => {
  const main = await source("src/main.tsx");

  assert.doesNotMatch(main, /from ["']\.\/App["']/);
  assert.doesNotMatch(main, /UpdateProvider|UpdateContext/);
  assert.doesNotMatch(main, /\?legacy|useLegacyShell/);
  assert.doesNotMatch(
    main,
    /syncModelsDevPricingOnStartup|MODELS_DEV_SYNC_CONFIG_QUERY_KEY/,
  );
  assert.match(main, /<AppRoot \/>/);
  assert.match(main, /@\/lib\/query\/queryClient/);
});

test("the product shell loads route families on demand", async () => {
  const [
    rootComponent,
    shell,
    routeLoading,
    routeTransition,
    routeMotion,
    barrel,
  ] = await Promise.all([
    source("src/app/AppRoot.tsx"),
    source("src/app/AppShell.tsx"),
    source("src/app/RouteLoadingFallback.tsx"),
    source("src/app/RouteContentTransition.tsx"),
    source("src/app/useRouteMotion.ts"),
    source("src/app/index.ts"),
  ]);

  assert.match(rootComponent, /lazy\(async \(\) => \(\{/);
  assert.match(rootComponent, /import\("\.\/AppShell"\)/);
  // The first screen is the shell itself: there is no first-run guide to route to.
  assert.doesNotMatch(rootComponent, /@\/pages\/onboarding/);
  assert.match(rootComponent, /<Suspense fallback=\{<StartupScreen \/>\}>/);

  for (const page of [
    "home/HomePage",
    "tools/ToolsPage",
    "services/ServicesPage",
    "extensions/ExtensionsPage",
    "data/DataPage",
    "settings/SettingsPage",
  ]) {
    assert.match(shell, new RegExp(`import\\(\\"@/pages/${page}\\"\\)`));
  }
  // ADR-0034: the merged areas load with their parent page, never as a route.
  for (const merged of [
    "routing/RoutingPage",
    "usage/UsagePage",
    "sessions/SessionsPage",
    "workspace",
  ]) {
    assert.doesNotMatch(shell, new RegExp(`@/pages/${merged}`));
  }
  assert.doesNotMatch(shell, /^import .* from ["']@\/pages\//m);
  assert.match(shell, /renderRoute=\{renderRoute\}/);
  assert.match(
    shell,
    /<Suspense fallback=\{<RouteLoadingFallback route=\{pageRoute\} \/>\}>/,
  );
  // One shared duration drives the rail indicator and both content layers.
  assert.match(routeMotion, /export const ROUTE_TRANSITION_MS = 1000/);
  assert.match(routeTransition, /ROUTE_TRANSITION_MS/);
  assert.doesNotMatch(
    routeTransition,
    /= *\d+;\s*\/\/ *ms|CONTENT_TRANSITION_MS/,
  );
  assert.match(routeTransition, /data-route-content-layer="outgoing"/);
  assert.match(routeTransition, /data-route-content-layer="incoming"/);
  assert.match(routeLoading, /aria-label=\{t\("nav\.loadingPage"\)\}/);
  assert.match(routeLoading, /ROUTE_LOADING_SHAPE\[route\]/);
  assert.match(routeLoading, /\sloading\s/);
  assert.doesNotMatch(barrel, /export \{ AppShell \}/);
});

test("the deleted legacy renderer islands cannot return silently", async () => {
  const boundaries = await source("scripts/check-boundaries.mjs");
  assert.doesNotMatch(boundaries, /['"]src\/App\.tsx['"]/);
  assert.doesNotMatch(boundaries, /['"]src\/lib\/api['"]/);
  assert.doesNotMatch(boundaries, /['"]src\/hooks\//);

  assert.deepEqual(await sourceFiles("src/components/"), [
    "DatabaseUpgrade.tsx",
    "FrontendErrorBoundary.tsx",
    "ui/button.tsx",
    "ui/sonner.tsx",
  ]);

  for (const path of ["src/hooks/", "src/lib/api/"]) {
    assert.deepEqual(await sourceFiles(path), []);
  }

  const [main, uuid, connectionFlow, ...presetSources] = await Promise.all([
    source("src/main.tsx"),
    source("src/utils/uuid.ts"),
    source("src/features/provider-management/useProviderConnectionFlow.ts"),
    ...[
      "claudeProviderPresets",
      "codexProviderPresets",
      "geminiProviderPresets",
      "grokBuildProviderPresets",
      "hermesProviderPresets",
      "openclawProviderPresets",
      "opencodeProviderPresets",
      "piProviderPresets",
    ].map((name) => source(`src/config/${name}.ts`)),
  ]);

  assert.match(main, /FrontendErrorBoundary/);
  assert.match(main, /import\("\.\/components\/DatabaseUpgrade"\)/);
  assert.match(uuid, /cryptoApi\.randomUUID/);
  assert.match(connectionFlow, /@\/utils\/uuid/);
  for (const contents of presetSources) assert.ok(contents.length > 0);
});

test("startup paints the one dark appearance the product ships", async () => {
  const [html, main] = await Promise.all([
    source("src/index.html"),
    source("src/main.tsx"),
  ]);

  // The document paints dark before any script runs, so the first frame never
  // flashes a light shell. There is no theme preference to read back.
  assert.match(html, /<html[^>]*class="dark"[^>]*color-scheme:\s*dark/);
  assert.doesNotMatch(html, /localStorage/);
  assert.doesNotMatch(main, /ThemeProvider|defaultTheme|forcedTheme/);

  // The native window is told the same appearance exactly once, through the
  // native boundary rather than a direct invoke.
  assert.match(main, /native\.system\.setWindowTheme\("dark"\)/);

  for (const contents of [html, main]) {
    assert.doesNotMatch(contents, /cc-switch-theme|ai-manager-theme/);
  }
});
