import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { crc32 } from "node:zlib";
import { describe, expect, it } from "vitest";

const ICON_DIR = resolve(process.cwd(), "src/assets/icons");
const PNG_MAGIC = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/**
 * Walks the PNG chunk stream and checks every CRC. A file whose header parses
 * but whose pixel data is truncated or invented still passes a magic-byte
 * check, renders as nothing in the app, and fails here.
 */
function pngChunkTypes(bytes: Buffer): string[] {
  expect(bytes.subarray(0, 8)).toEqual(PNG_MAGIC);
  const types: string[] = [];
  let offset = 8;
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.subarray(offset + 4, offset + 8).toString("ascii");
    const body = bytes.subarray(offset + 4, offset + 8 + length);
    expect(crc32(body)).toBe(bytes.readUInt32BE(offset + 8 + length));
    types.push(type);
    offset += 12 + length;
  }
  expect(offset).toBe(bytes.length);
  return types;
}

const files = readdirSync(ICON_DIR).filter((name) => /\.(png|svg)$/.test(name));

describe("bundled app icon assets", () => {
  it("ships icons at all", () => {
    expect(files.length).toBeGreaterThan(0);
  });

  it.each(files)("%s decodes as a complete image", (name) => {
    const bytes = readFileSync(join(ICON_DIR, name));
    if (name.endsWith(".png")) {
      const types = pngChunkTypes(bytes);
      expect(types[0]).toBe("IHDR");
      expect(types).toContain("IDAT");
      expect(types.at(-1)).toBe("IEND");
      return;
    }
    const source = bytes.toString("utf8");
    expect(source).toContain("<svg");
    // An SVG that only wraps a raster payload is still a raster asset: the
    // wrapper renders fine while the payload inside it is unusable.
    for (const [, encoded] of source.matchAll(
      /href="data:image\/png;base64,([^"]+)"/g,
    )) {
      pngChunkTypes(Buffer.from(encoded.replace(/\s/g, ""), "base64"));
    }
  });
});
