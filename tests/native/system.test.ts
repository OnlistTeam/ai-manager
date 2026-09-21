import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import {
  CONFIG_LOAD_ERROR_EVENT,
  NativeError,
  native,
  onConfigLoadError,
} from "@/native";
import { server } from "../msw/server";
import { emitTauriEvent } from "../msw/tauriMocks";

const TAURI_ENDPOINT = "http://tauri.local";

const RECORDED = {
  path: "/data/config.json",
  error: "database schema is newer than this build",
  kind: "db_version_too_new",
  db_version: 21,
  supported_version: 19,
};

describe("native.system.initError", () => {
  it("reads a clean start as null", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/get_init_error`, () =>
        HttpResponse.json(null),
      ),
    );
    await expect(native.system.initError()).resolves.toBeNull();
  });

  it("reads the recorded startup failure", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/get_init_error`, () =>
        HttpResponse.json(RECORDED),
      ),
    );
    await expect(native.system.initError()).resolves.toEqual(RECORDED);
  });

  it("rejects a payload that does not match the schema instead of guessing a branch", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/get_init_error`, () =>
        HttpResponse.json({ kind: 42 }),
      ),
    );
    const failure = native.system.initError();
    await expect(failure).rejects.toBeInstanceOf(NativeError);
    await expect(failure).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });
});

describe("onConfigLoadError", () => {
  it("hands a well-formed payload to the listener", async () => {
    const listener = vi.fn();
    const stop = await onConfigLoadError(listener);
    emitTauriEvent(CONFIG_LOAD_ERROR_EVENT, RECORDED);
    expect(listener).toHaveBeenCalledWith(RECORDED);
    stop();
  });

  it("still raises the alarm, with no detail, when the payload is malformed", async () => {
    const listener = vi.fn();
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const stop = await onConfigLoadError(listener);
    emitTauriEvent(CONFIG_LOAD_ERROR_EVENT, { path: 7 });
    expect(listener).toHaveBeenCalledWith(null);
    expect(error).toHaveBeenCalled();
    error.mockRestore();
    stop();
  });
});
