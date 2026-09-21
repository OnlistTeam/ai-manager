import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import test from "node:test";

const root = new URL("../", import.meta.url);
const upstreamDigests = new Set([
  "04225b1b9c54569ec1ec850ad9f1c9f33ca4f286dab001a3392c0460deb342e5",
  "f18645510570b45df07f8b785ff7ee979f4c7c1dd828276053875292313e3ec0",
  "8c167919ba52ed97aaa5b612819821a0c6dba2c7cd2c03a09df0a2086451d2aa",
  "65c365e7c32929807deec2d7469eed2d5c06f21ce86854a4848057d2df588ab8",
  "737004c91a766a67e57a62f695a28e2390a6fbd48efcb1d3eda1fe5bf289c3f4",
  "c82ff0ffb2800fbcbbdd88b92d1844527ef5fb40eb5d6fb3213bab505b2ee007",
]);

async function bytes(path) {
  return readFile(new URL(path, root));
}

function pngHeader(contents) {
  assert.equal(contents.subarray(1, 4).toString("ascii"), "PNG");
  assert.equal(contents.subarray(12, 16).toString("ascii"), "IHDR");
  return {
    width: contents.readUInt32BE(16),
    height: contents.readUInt32BE(20),
    colorType: contents[25],
  };
}

test("brand masters and product-facing rasters have fixed dimensions", async () => {
  const expected = new Map([
    ["src/assets/spatial/models/v5/app-icon.png", [1024, 1024]],
    ["src-tauri/icons/icon.png", [512, 512]],
    ["src-tauri/icons/128x128.png", [128, 128]],
    ["src-tauri/icons/32x32.png", [32, 32]],
    ["src-tauri/icons/dmg-background.png", [660, 400]],
    ["src-tauri/icons/dmg-background@2x.png", [1320, 800]],
    ["src-tauri/icons/tray/macos/statusbar_template_3x.png", [72, 72]],
    ["src/assets/icons/app-icon.png", [32, 32]],
  ]);

  for (const [path, [width, height]] of expected) {
    const header = pngHeader(await bytes(path));
    assert.deepEqual([header.width, header.height], [width, height], path);
  }
});

test("the icon master and macOS tray raster preserve an alpha channel", async () => {
  for (const path of [
    "src/assets/spatial/models/v5/app-icon.png",
    "src-tauri/icons/tray/macos/statusbar_template_3x.png",
  ]) {
    const { colorType } = pngHeader(await bytes(path));
    assert.equal(colorType, 6, path);
  }
});

test("every Tauri bundle icon path exists", async () => {
  const config = JSON.parse(
    (await bytes("src-tauri/tauri.conf.json")).toString("utf8"),
  );
  for (const path of config.bundle.icon) {
    assert((await bytes(`src-tauri/${path}`)).length > 0, path);
  }
});

test("the renderer product mark matches the small bundle icon", async () => {
  assert.deepEqual(
    await bytes("src/assets/icons/app-icon.png"),
    await bytes("src-tauri/icons/32x32.png"),
  );
});

test("the ICNS container uses canonical chunk ordering", async () => {
  const contents = await bytes("src-tauri/icons/icon.icns");
  assert.equal(contents.subarray(0, 4).toString("ascii"), "icns");
  assert.equal(contents.readUInt32BE(4), contents.length);

  const types = [];
  let offset = 8;
  while (offset < contents.length) {
    const length = contents.readUInt32BE(offset + 4);
    assert(length >= 8 && offset + length <= contents.length);
    types.push(contents.subarray(offset, offset + 4).toString("ascii"));
    offset += length;
  }
  assert.deepEqual(types, [...types].sort());
});

test("no product-facing brand raster is byte-identical to upstream", async () => {
  for (const path of [
    "src-tauri/icons/icon.png",
    "src-tauri/icons/dmg-background.png",
    "src-tauri/icons/dmg-background@2x.png",
    "src-tauri/icons/tray/macos/statusbar_template_3x.png",
    "src/assets/icons/app-icon.png",
  ]) {
    const digest = createHash("sha256")
      .update(await bytes(path))
      .digest("hex");
    assert(!upstreamDigests.has(digest), path);
  }
});
