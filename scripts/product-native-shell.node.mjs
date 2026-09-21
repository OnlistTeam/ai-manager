import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function source(path) {
  return readFile(new URL(path, root), "utf8");
}

function functionSource(contents, signature, nextSignature) {
  const start = contents.indexOf(signature);
  const end = contents.indexOf(nextSignature, start + signature.length);
  assert.notEqual(start, -1, `missing function signature: ${signature}`);
  assert.notEqual(end, -1, `missing next function signature: ${nextSignature}`);
  return contents.slice(start, end);
}

function commandsMatching(contents, pattern) {
  return [...contents.matchAll(pattern)].map((match) => match[1]);
}

test("product writes cannot fall through to the CC Switch compatibility directory", async () => {
  const [paths, config, appConfig, settings] = await Promise.all([
    source("src-tauri/src/infrastructure/paths.rs"),
    source("src-tauri/src/config.rs"),
    source("src-tauri/src/app_config.rs"),
    source("src-tauri/src/settings.rs"),
  ]);

  const productDataDir = functionSource(
    paths,
    "pub fn product_data_dir()",
    "pub fn app_db_path()",
  );
  assert.doesNotMatch(productDataDir, /app_store|get_app_config_dir/);
  assert.match(productDataDir, /default_product_data_dir/);
  assert.doesNotMatch(productDataDir, /\.cc-switch/);

  const ccSwitchDatabase = functionSource(
    paths,
    "pub fn cc_switch_db_path()",
    "#[cfg(test)]",
  );
  assert.match(ccSwitchDatabase, /get_default_cc_switch_config_dir/);
  assert.doesNotMatch(ccSwitchDatabase, /get_app_config_dir\(\)/);

  const productConfigPath = functionSource(
    config,
    "pub fn get_app_config_path()",
    "pub fn read_json_file",
  );
  assert.match(productConfigPath, /product_data_dir/);
  assert.doesNotMatch(productConfigPath, /get_app_config_dir/);

  const productSettingsPath = functionSource(
    settings,
    "    fn settings_path()",
    "    fn normalize_paths",
  );
  assert.match(productSettingsPath, /product_data_dir/);
  assert.doesNotMatch(productSettingsPath, /\.cc-switch/);

  const save = functionSource(
    appConfig,
    "    pub fn save(&self)",
    "    pub fn get_manager(&self",
  );
  assert.doesNotMatch(save, /get_app_config_dir/);

  for (const pathOwner of [
    "src-tauri/src/codex_config.rs",
    "src-tauri/src/codex_history_migration.rs",
    "src-tauri/src/commands/config.rs",
    "src-tauri/src/hermes_config.rs",
    "src-tauri/src/openclaw_config.rs",
    "src-tauri/src/services/model_pricing.rs",
    "src-tauri/src/services/proxy.rs",
    "src-tauri/src/services/skill.rs",
    "src-tauri/src/store.rs",
  ]) {
    assert.doesNotMatch(
      await source(pathOwner),
      /get_app_config_dir/,
      pathOwner,
    );
  }
});

test("the stable identifier migration is read-only, allow-listed, and ordered before database init", async () => {
  const [migration, lib, tauriConfigSource] = await Promise.all([
    source("src-tauri/src/database/product_data_migration.rs"),
    source("src-tauri/src/lib.rs"),
    source("src-tauri/tauri.conf.json"),
  ]);
  const productionMigration = migration.split("#[cfg(test)]", 1)[0];
  const tauriConfig = JSON.parse(tauriConfigSource);

  assert.equal(tauriConfig.identifier, "tools.aimanager.desktop");
  assert.match(
    productionMigration,
    /LEGACY_PRODUCT_IDENTIFIER:\s*&str\s*=\s*"com\.ccswitch\.desktop"/,
  );
  assert.match(productionMigration, /\["skills",\s*"skill-backups"\]/);
  assert.match(productionMigration, /SQLITE_OPEN_READ_ONLY/);
  assert.match(productionMigration, /Backup::new/);
  assert.match(productionMigration, /persist_noclobber/);
  assert.match(productionMigration, /symlink_metadata/);
  assert.doesNotMatch(productionMigration, /remove_(?:file|dir)|\brename\s*\(/);
  for (const excluded of [
    "app_paths.json",
    ".window-state.json",
    "settings.json",
    "config.json",
    "codex_oauth_auth.json",
    "logs",
  ]) {
    assert.doesNotMatch(
      productionMigration,
      new RegExp(excluded.replace(".", "\\.")),
    );
  }

  const migrationCall = lib.indexOf("migrate_legacy_product_data");
  const versionPreflight = lib.indexOf("stored_user_version_exceeds_supported");
  const databaseInit = lib.indexOf("crate::database::Database::init()");
  assert(migrationCall >= 0);
  assert(versionPreflight > migrationCall);
  assert(databaseInit > versionPreflight);
});

test("the product registers its own deep link and never the inherited one", async () => {
  const [tauriConfigSource, infoPlist, cargo, lib, wix] = await Promise.all([
    source("src-tauri/tauri.conf.json"),
    source("src-tauri/Info.plist"),
    source("src-tauri/Cargo.toml"),
    source("src-tauri/src/lib.rs"),
    source("src-tauri/wix/per-user-main.wxs"),
  ]);
  const tauriConfig = JSON.parse(tauriConfigSource);

  // ADR-0029 decision 1: exactly one scheme, and it is the product's own.
  // Co-registering `ccswitch` would hijack the import links of users who also
  // have the upstream application installed.
  assert.deepEqual(tauriConfig.plugins["deep-link"], {
    desktop: { schemes: ["aimanager"] },
  });
  assert.doesNotMatch(tauriConfigSource, /ccswitch/i);
  // The bundler generates the macOS URL types from the config above; a
  // hand-written entry here would be a second, drifting source.
  assert.doesNotMatch(infoPlist, /CFBundleURLTypes|ccswitch/i);
  // The Windows installer writes the protocol from the same configuration.
  assert.match(wix, /\{\{#each deep_link_protocols as \|protocol\|/);

  // Pinned exactly, with the core framework pinned alongside it (AI_RULES 5).
  assert.match(cargo, /tauri-plugin-deep-link = "=2\.4\.10"/);
  assert.match(cargo, /^tauri = \{ version = "=2\.10\.3"/m);
  assert.match(
    cargo,
    /tauri-plugin-single-instance = \{ version = "2", features = \["deep-link"\] \}/,
  );
  assert.doesNotMatch(cargo, /tauri-plugin-store/);

  // Single instance is registered first, as the plugin requires, so a second
  // launch reaches the same parser instead of leaving the URL in argv.
  const singleInstance = lib.indexOf("tauri_plugin_single_instance::init");
  const deepLink = lib.indexOf("tauri_plugin_deep_link::init");
  assert(singleInstance >= 0 && deepLink > singleInstance);
  assert.match(lib, /fn register_deep_link_handling/);
  assert.match(lib, /\.on_open_url\(/);
  assert.match(lib, /LinkOrigin::Argv/);
  assert.match(lib, /commands::app_deeplink_confirm/);
  // Only the renderer's own paste may carry a credential (decision 3/4).
  assert.doesNotMatch(lib, /LinkOrigin::Paste/);
  assert.doesNotMatch(
    lib,
    /tauri_plugin_store|app_store|handle_deeplink_url|commands::(?:parse_deeplink|merge_deeplink_config|import_from_deeplink|enter_lightweight_mode|exit_lightweight_mode|is_lightweight_mode|get_app_config_dir_override|set_app_config_dir_override)/,
  );
});

test("Open Tool keeps project paths in the native safe-launch chain", async () => {
  const [
    appApi,
    lib,
    nativeTools,
    toolsPage,
    homePage,
    launchFlow,
    servicesStartAction,
    servicesPage,
    terminal,
    permissions,
    infoPlist,
  ] = await Promise.all([
    source("src-tauri/src/commands/app_api.rs"),
    source("src-tauri/src/lib.rs"),
    source("src/native/commands/tools.ts"),
    source("src/pages/tools/ToolsPage.tsx"),
    source("src/pages/home/HomePage.tsx"),
    source("src/features/tool-management/useToolLaunchFlow.ts"),
    source("src/pages/services/ServicesStartAction.tsx"),
    source("src/pages/services/ServicesEndpointsPanel.tsx"),
    source("src-tauri/src/platform/terminal.rs"),
    source("src-tauri/capabilities/default.json"),
    source("src-tauri/Info.plist"),
  ]);

  const command = functionSource(
    appApi,
    "pub async fn app_tool_launch(",
    "pub async fn app_providers_list(",
  );
  assert.match(command, /blocking_pick_folder/);
  assert.match(command, /ToolLaunchService::system\(\)\s*\.launch/);
  assert.match(command, /ToolLaunchOutcome::Cancelled/);
  assert.match(lib, /commands::app_tool_launch/);

  // The renderer submits only the stable tool id and a fixed directory-mode
  // enum ("default" | "choose"); the folder itself is picked natively.
  const client = functionSource(
    nativeTools,
    "  launch(",
    "  uninstallPreview(",
  );
  assert.match(client, /app_tool_launch/);
  assert.match(
    client,
    /tool,\s*directoryMode:\s*toolLaunchDirectoryModeSchema\.parse\(directoryMode\)/,
  );
  assert.doesNotMatch(
    client,
    /projectPath|workingDirectory|selectedPath|directoryPath/,
  );

  const dispatch = functionSource(
    toolsPage,
    "  function handleAction(",
    "  function dismissFailure(",
  );
  assert.match(dispatch, /case "open":\s+launch\.openTool\(tool\)/);
  const openCase = functionSource(
    dispatch,
    '      case "open":',
    '      case "fix":',
  );
  assert.doesNotMatch(openCase, /tools\.action\.unavailable/);
  assert.match(launchFlow, /useLaunchTool\(\{\s*notifyOnError:\s*false\s*\}\)/);
  assert.match(launchFlow, /launch\.mutate\(\s*\{\s*tool:\s*selected\.id/);
  assert.match(launchFlow, /error:\s*launch\.error/);
  assert.match(launchFlow, /launch\.reset\(\)/);
  assert.doesNotMatch(
    launchFlow,
    /projectPath|workingDirectory|selectedPath|directoryPath/,
  );
  // One launch flow per endpoints panel feeds both the Start button and the
  // post-switch "Open now" action (ADR-0032); the action component owns no hook.
  assert.match(servicesPage, /useToolLaunchFlow\(\)/);
  assert.match(servicesPage, /onOpenTool:/);
  assert.doesNotMatch(servicesStartAction, /useToolLaunchFlow\(\)/);
  assert.match(servicesStartAction, /launch\.openTool\(tool\)/);
  assert.match(homePage, /useToolLaunchFlow\(\)/);
  assert.match(homePage, /firstLaunchableTool\(list\)/);
  assert.match(homePage, /<OpenToolModal/);
  assert.match(homePage, /onStart=\{launch\.openTool\}/);
  assert.doesNotMatch(
    homePage,
    /projectPath|workingDirectory|selectedPath|directoryPath/,
  );

  assert.match(terminal, /const MACOS_TERMINAL_SCRIPT: &str/);
  assert.match(terminal, /const WSL_TERMINAL_SCRIPT: &str/);
  assert.match(terminal, /let sensitive = \(1\.\.args\.len\(\)\)\.collect\(\)/);
  assert.match(terminal, /quoted form of \(item argument_index of argv\)/);
  assert.match(terminal, /MAX_TERMINAL_ARGS/);
  assert.match(terminal, /AI_MANAGER_TOOL/);
  assert.doesNotMatch(permissions, /dialog:allow-open|dialog:allow-read/);
  assert.match(infoPlist, /NSAppleEventsUsageDescription/);
});

test("the native product tray keeps quick switching inside the product boundary", async () => {
  const [lib, tray] = await Promise.all([
    source("src-tauri/src/lib.rs"),
    source("src-tauri/src/tray.rs"),
  ]);

  assert.match(lib, /\.tooltip\("AI Manager"\)/);
  assert.doesNotMatch(lib, /refresh_all_usage_in_tray/);
  assert.doesNotMatch(lib, /please upgrade CC Switch/i);

  assert.match(tray, /TRAY_ID:\s*&str\s*=\s*"ai-manager"/);
  assert.match(tray, /ProductSettingsService::load/);
  assert.match(tray, /ProviderDirectory::(?:list|switch)/);
  assert.match(tray, /provider_fingerprint/);
  assert.match(tray, /PROVIDER_CHANGED_EVENT/);
  assert.doesNotMatch(tray, /format!\([^\n]*provider\.id/);
  assert.doesNotMatch(
    tray,
    /ccswitch\.io|open_website|lightweight_mode|TRAY_SECTIONS|queryProviderUsage/,
  );
});

test("product startup does not activate post-MVP background services", async () => {
  const lib = await source("src-tauri/src/lib.rs");

  assert.doesNotMatch(
    lib,
    /(?:webdav_auto_sync|s3_auto_sync)::start_worker/,
    "hidden cloud-sync workers must not run in the local-first product shell",
  );
  assert.doesNotMatch(
    lib,
    /restore_proxy_state_on_startup/,
    "hidden proxy takeover must not be restored by product startup",
  );
  assert.doesNotMatch(
    lib,
    /session_usage::(?:session_sync_mutex|sync_all_unlocked)|backfill_missing_usage_costs/,
    "hidden usage/session collection must not run in the product background",
  );
  assert.doesNotMatch(
    lib,
    /maybe_migrate_codex_official_history_to_unified_bucket/,
    "hidden session-history migration must not run during product startup",
  );
});

test("native IPC exposes exactly the production renderer command contract", async () => {
  const commandDirectory = new URL("src/native/commands/", root);
  const commandFiles = (await readdir(commandDirectory)).filter((name) =>
    name.endsWith(".ts"),
  );
  const commandSources = await Promise.all(
    commandFiles.map((name) => source(`src/native/commands/${name}`)),
  );
  commandSources.push(await source("src/native/updater.ts"));
  // The files the boundary checker allows to call Tauri directly, so their
  // commands count towards the contract just like the native client's.
  const directSources = await Promise.all([
    source("src/main.tsx"),
    source("src/components/DatabaseUpgrade.tsx"),
    source("src/lib/frontendLogger.ts"),
    source("src/lib/windowActivity.ts"),
  ]);
  const expected = new Set([
    ...commandSources.flatMap((contents) =>
      commandsMatching(contents, /invokeNative\(\s*["']([^"']+)["']/g),
    ),
    ...directSources.flatMap((contents) =>
      commandsMatching(contents, /\binvoke\(\s*["']([^"']+)["']/g),
    ),
  ]);

  const lib = await source("src-tauri/src/lib.rs");
  const handler = functionSource(
    lib,
    "        .invoke_handler(tauri::generate_handler![",
    "        ]);",
  );
  const registered = new Set(
    commandsMatching(
      handler,
      /^\s*(?:commands::)?([A-Za-z_][A-Za-z0-9_]*),\s*$/gm,
    ),
  );

  assert.deepEqual([...registered].sort(), [...expected].sort());
});

test("the production webview is branded and local-only", async () => {
  const [tauriConfigSource, html, cargo] = await Promise.all([
    source("src-tauri/tauri.conf.json"),
    source("src/index.html"),
    source("src-tauri/Cargo.toml"),
  ]);
  const tauriConfig = JSON.parse(tauriConfigSource);
  const security = tauriConfig.app.security;
  const inlineScripts = [...html.matchAll(/<script>([\s\S]*?)<\/script>/g)];

  assert.match(html, /<title>AI Manager<\/title>/);
  assert.equal(inlineScripts.length, 0);
  assert.equal(
    security.csp,
    "default-src 'self'; base-uri 'none'; form-action 'none'; frame-src 'none'; object-src 'none'; img-src 'self' data:; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' ipc: http://ipc.localhost",
  );
  assert.deepEqual(security.assetProtocol, { enable: false, scope: [] });
  assert.doesNotMatch(cargo, /protocol-asset/);
});

test("the native product window keeps the tested brand and layout envelope", async () => {
  const [baseSource, windowsSource] = await Promise.all([
    source("src-tauri/tauri.conf.json"),
    source("src-tauri/tauri.windows.conf.json"),
  ]);
  const baseWindow = JSON.parse(baseSource).app.windows[0];
  const windowsWindow = JSON.parse(windowsSource).app.windows[0];
  const expectedGeometry = {
    width: 1180,
    height: 760,
    minWidth: 900,
    minHeight: 620,
  };

  assert.deepEqual(
    {
      width: baseWindow.width,
      height: baseWindow.height,
      minWidth: baseWindow.minWidth,
      minHeight: baseWindow.minHeight,
    },
    expectedGeometry,
  );
  assert.equal(baseWindow.label, "main");
  assert.equal(baseWindow.title, "");
  assert.equal(baseWindow.titleBarStyle, "Overlay");
  assert.equal(baseWindow.visible, false);
  assert.equal(baseWindow.resizable, true);
  assert.equal(baseWindow.fullscreen, false);
  assert.equal(baseWindow.center, true);

  // Tauri replaces platform-specific array entries instead of merging their
  // fields, so Windows must repeat every intended window property explicitly.
  // `titleBarStyle` is deliberately absent: it is a macOS-only key, so setting
  // it here described a Windows behaviour that never existed. The caption
  // colour comes from platform::window_chrome instead.
  assert.deepEqual(windowsWindow, {
    label: "main",
    title: "AI Manager",
    ...expectedGeometry,
    visible: false,
    resizable: true,
    fullscreen: false,
    center: true,
  });
  assert.doesNotMatch(windowsSource, /CC Switch/);
});

test("the product updater uses only its narrow signed HTTPS channels", async () => {
  const [
    lib,
    updater,
    manager,
    channel,
    retry,
    build,
    recovery,
    capability,
    config,
    workflow,
  ] = await Promise.all([
    source("src-tauri/src/lib.rs"),
    source("src/native/updater.ts"),
    source("src-tauri/src/application/app_update.rs"),
    source("src-tauri/src/application/app_update/channel.rs"),
    source("src-tauri/src/application/app_update/retry.rs"),
    source("src-tauri/build.rs"),
    source("src/components/DatabaseUpgrade.tsx"),
    source("src-tauri/capabilities/default.json"),
    source("src-tauri/tauri.conf.json"),
    source(".github/workflows/release.yml"),
  ]);

  assert.match(updater, /app_update_start/);
  assert.match(updater, /app_update_install_and_restart/);
  assert.match(updater, /app_update_open_download_page/);
  assert.doesNotMatch(updater, /@tauri-apps\/plugin-updater|\bcheck\s*\(/);
  assert.match(manager, /!status\.channel_ready/);
  assert.match(manager, /retryable_updater_error/);
  assert.match(manager, /endpoint_order/);
  assert.match(manager, /signed update disappeared between automatic retries/);
  assert.match(manager, /download\(/);
  assert.match(channel, /endpoint\.scheme\(\) != "https"/);
  assert.match(channel, /dangerous_accept_invalid_certs/);
  assert.match(channel, /option_env!\("AI_MANAGER_UPDATE_CHANNEL_ENABLED"\)/);
  assert.match(channel, /AI_MANAGER_UPDATE_CHANNEL/);
  assert.match(
    channel,
    /dl\.aimanager\.tools\/ai-manager\/staging\/latest\.json/,
  );
  assert.match(retry, /MAX_ATTEMPTS:\s*u8\s*=\s*3/);
  assert.match(retry, /Error::Minisign/);
  assert.match(build, /AI_MANAGER_UPDATE_CHANNEL=stable or staging/);
  assert.match(workflow, /AI_MANAGER_UPDATE_CHANNEL_ENABLED:\s*"1"/);
  assert.match(
    workflow,
    /AI_MANAGER_UPDATE_CHANNEL:\s*\$\{\{ inputs\.prerelease && 'staging' \|\| 'stable' \}\}/,
  );
  assert.doesNotMatch(
    recovery,
    /farion1231|ccswitch\.io|check_app_update_available|install_update_and_restart/,
  );
  assert.match(lib, /tauri_plugin_updater::Builder/);
  assert.match(lib, /commands::app_update_start/);
  assert.doesNotMatch(
    lib,
    /commands::(?:check_for_updates|check_app_update_available|install_update_and_restart)/,
  );
  assert.deepEqual(JSON.parse(config).plugins.updater.endpoints, [
    "https://dl.aimanager.tools/ai-manager/latest.json",
  ]);
  assert.doesNotMatch(
    capability,
    /updater:/,
    "the main window must not receive generic updater IPC permission",
  );
});

test("the main-window capability matches the production renderer exactly", async () => {
  const [
    capability,
    main,
    events,
    recovery,
    frontendLogger,
    updater,
    windowActivity,
    appShell,
  ] = await Promise.all([
    source("src-tauri/capabilities/default.json"),
    source("src/main.tsx"),
    source("src/native/events.ts"),
    source("src/components/DatabaseUpgrade.tsx"),
    source("src/lib/frontendLogger.ts"),
    source("src/native/updater.ts"),
    source("src/lib/windowActivity.ts"),
    source("src/app/AppShell.tsx"),
  ]);
  const permissions = JSON.parse(capability).permissions;

  // Event listening lives in the native boundary; the entry only consumes it.
  assert.match(events, /@tauri-apps\/api\/event/);
  assert.doesNotMatch(main, /@tauri-apps\/api\/event/);
  assert.match(main, /@tauri-apps\/plugin-dialog/);
  assert.match(main + recovery, /@tauri-apps\/plugin-process/);
  assert.match(frontendLogger, /@tauri-apps\/plugin-log/);
  assert.doesNotMatch(updater, /@tauri-apps\/api\/app/);
  assert.match(windowActivity, /@tauri-apps\/api\/(?:event|window)/);
  assert.match(appShell, /data-tauri-drag-region|DRAG_REGION_ATTR/);

  assert.deepEqual([...permissions].sort(), [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "core:window:allow-internal-toggle-maximize",
    "core:window:allow-start-dragging",
    "dialog:allow-message",
    "log:allow-log",
    "process:allow-exit",
  ]);
});
