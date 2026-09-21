import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);

async function source(path) {
  return readFile(new URL(path, root), "utf8");
}

test("product logging has a bounded AI Manager contract", async () => {
  const logging = await source("src-tauri/src/infrastructure/logging.rs");

  assert.match(logging, /MAIN_LOG_FILE_STEM:\s*&str\s*=\s*"ai-manager"/);
  assert.match(
    logging,
    /MAIN_LOG_MAX_SIZE:\s*u128\s*=\s*10\s*\*\s*1024\s*\*\s*1024/,
  );
  assert.match(logging, /MAIN_LOG_ARCHIVES_TO_KEEP:\s*usize\s*=\s*4/);
  assert.match(logging, /join\("logs"\)/);
  assert.match(logging, /join\("crash\.log"\)/);
});

test("startup resolves AppData before installing file logging and panic capture", async () => {
  const lib = await source("src-tauri/src/lib.rs");
  const appData = lib.indexOf("app.path().app_data_dir()");
  const productPath = lib.indexOf("paths::init_product_data_dir");
  const panicHook = lib.indexOf("panic_hook::setup_panic_hook()");
  const fileLogger = lib.indexOf("tauri_plugin_log::Builder::default()");

  assert(appData >= 0);
  assert(productPath > appData);
  assert(panicHook > productPath);
  assert(fileLogger > panicHook);
  assert.doesNotMatch(lib, /CC Switch v\{\}/);
  assert.doesNotMatch(lib, /file_name:\s*Some\("cc-switch"/);
  assert.match(lib, /log::set_max_level\(log::LevelFilter::Info\)/);
});

test("panic and recovery fallbacks never point at the CC Switch directory", async () => {
  const [panicHook, main, appConfig] = await Promise.all([
    source("src-tauri/src/panic_hook.rs"),
    source("src/main.tsx"),
    source("src-tauri/src/app_config.rs"),
  ]);

  const panicProduction = panicHook.split("#[cfg(test)]", 1)[0];
  const appConfigProduction = appConfig.split("#[cfg(test)]", 1)[0];
  assert.doesNotMatch(panicProduction, /\.cc-switch|\[CC-Switch\]/);
  assert.match(panicProduction, /redact_secrets/);
  assert.match(panicProduction, /CRASH_ENTRY_MAX_SIZE/);
  assert.doesNotMatch(panicProduction, /default_hook\(panic_info\)/);
  assert.doesNotMatch(main, /~\/\.cc-switch\/config\.json/);
  assert.doesNotMatch(appConfigProduction, /~\/\.cc-switch\/config\.json/);
});

test("no shipped locale names a diagnostic log the product does not write", async () => {
  for (const locale of ["en", "ja", "zh", "zh-TW"]) {
    const contents = await source(`src/i18n/locales/${locale}.json`);
    // The product writes logs/ai-manager.log. Copy may stay silent about the
    // file, but whenever it names one it must be that file.
    const named = [...contents.matchAll(/[A-Za-z0-9_-]+\.log\b/g)].map(
      ([match]) => match,
    );
    assert.deepEqual(
      [...new Set(named)].filter((name) => name !== "ai-manager.log"),
      [],
      locale,
    );
  }
});
