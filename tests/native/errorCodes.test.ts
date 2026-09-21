import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { errorCodeSchema } from "@/native";

/**
 * Rust's ErrorCode and the frontend's zod enum are two halves of the same
 * contract. If either side adds a value the other doesn't keep up with, the
 * frontend will treat a valid backend error as a schema mismatch, which
 * pulls the rug out from under the §42 branch.
 */
describe("error code parity", () => {
  it("has exactly the codes the Rust enum can produce", () => {
    const source = readFileSync("src-tauri/src/domain/error.rs", "utf8");
    const fromMatchArm = [
      ...source.matchAll(/Self::(\w+) => "([A-Z_]+)",/g),
    ].map((match) => match[2]);

    expect(fromMatchArm.length).toBeGreaterThan(0);
    expect([...errorCodeSchema.options].sort()).toEqual(
      [...fromMatchArm].sort(),
    );
  });

  it("gives every declared variant a wire string", () => {
    // The test above only looks at the `as_str()` match arms. If a new enum
    // variant forgets its match arm, it simply doesn't exist in that test, so
    // both sides still compare equal. So here we pull the variant names from
    // the declaration block instead and diff them against the left side of
    // the match arms: a missing match arm shows up here.
    const source = readFileSync("src-tauri/src/domain/error.rs", "utf8");
    const start = source.indexOf("pub enum ErrorCode {");
    const body = source.slice(start, source.indexOf("\n}", start));
    const declared = [...body.matchAll(/^\s{4}(\w+),$/gm)].map(
      (match) => match[1],
    );
    const mapped = [...source.matchAll(/Self::(\w+) => "[A-Z_]+",/g)].map(
      (match) => match[1],
    );

    expect(declared.length).toBeGreaterThan(0);
    expect([...declared].sort()).toEqual([...mapped].sort());
  });
});
