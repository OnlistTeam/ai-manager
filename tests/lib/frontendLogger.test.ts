const writeErrorLog = vi.hoisted(() =>
  vi.fn<(message: string, options: { file: string }) => Promise<void>>(() =>
    Promise.resolve(),
  ),
);

vi.mock("@tauri-apps/plugin-log", () => ({
  error: writeErrorLog,
}));

import {
  installGlobalErrorHandlers,
  redactFrontendLogText,
  reportFrontendError,
} from "@/lib/frontendLogger";

describe("frontendLogger", () => {
  beforeEach(() => {
    writeErrorLog.mockClear();
  });

  it("redacts URL parameters and named credentials", () => {
    const redacted = redactFrontendLogText(
      "https://example.test/path?apiKey=query-secret&name=alice\n" +
        'api_key: "config secret with spaces"\n' +
        "Authorization: Bearer bearer-secret\n" +
        "Authorization: Basic dXNlcjpwYXNz\n" +
        "Authorization: Token token-secret\n" +
        "  Cookie: session=cookie-secret; preference=private\n" +
        "ApiKey standalone-secret\n" +
        "https://user:password@example.test/private",
    );

    expect(redacted).not.toContain("query-secret");
    expect(redacted).not.toContain("alice");
    expect(redacted).not.toContain("config secret with spaces");
    expect(redacted).not.toContain("secret with spaces");
    expect(redacted).not.toContain("bearer-secret");
    expect(redacted).not.toContain("dXNlcjpwYXNz");
    expect(redacted).not.toContain("token-secret");
    expect(redacted).not.toContain("standalone-secret");
    expect(redacted).not.toContain("cookie-secret");
    expect(redacted).not.toContain("preference=private");
    expect(redacted).not.toContain("user:password");
    expect(redacted).toContain("apiKey=[REDACTED]");
    expect(redacted).toContain('api_key: "[REDACTED]"');

    const escapedMultiline = redactFrontendLogText(
      '{"detail":"line1\\nsk-ant-api03-escaped-secret"}',
    );
    expect(escapedMultiline).not.toContain("sk-ant-api03-escaped-secret");
  });

  it("writes bounded, redacted errors through the Tauri log plugin", () => {
    const secret = "sensitive-token";
    const oversizedDetails = `${"x".repeat(2_000_000)} token=${secret}`;

    reportFrontendError(
      "window.error",
      new Error(`failed at https://example.test/?key=${secret}`),
      oversizedDetails,
    );

    expect(writeErrorLog).toHaveBeenCalledOnce();
    const [message, options] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain(secret);
    expect(message).toContain("[truncated]");
    expect(message.length).toBeLessThanOrEqual(12_020);
    expect(options).toEqual({ file: "frontend" });
  });

  it("serializes object-shaped rejection reasons without exposing secrets", () => {
    // Two-tier redaction contract:
    //  - Property-name tier: a match on a sensitive name (including
    //    plural/array/nested) hides the entire value unconditionally;
    //  - Value tier: an "opaque secret shape" anywhere in the value is
    //    hidden based on its shape;
    //  - Text tier: after serialization, a bare secret in `"name":"value"`
    //    form is caught by a regex as a fallback.
    reportFrontendError("unhandledrejection", {
      code: 500,
      message: "auth failed", // benign text kept as-is (value contains "auth" but isn't name:value)
      key: "short-secret", // bare key scalar (not opaque) → property-name tier
      token: "object-secret", // named scalar → property-name tier
      tokens: ["k-9f3a7c2b1e"], // plural + array (short value) → property-name tier (strip trailing s + hide the whole thing)
      auth: ["opaque-credential"], // array → property-name tier
      credential: "AIzaRealCredential123",
      nested: { detail: "ghp_abcdef123456" }, // non-sensitive name, caught by the opaque shape
      values: ["eyJhbGciOiJIUzI1NiJ9.cGF5bG9hZA.c2lnbmF0dXJl"], // opaque shape inside an array
      multiline: "line1\nsk-ant-api03-multiline-secret", // opaque shape inside a string
      session: { activeTab: "providers", scrollPos: 120 }, // benign state kept as-is
    });

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).toContain('"code":500');
    expect(message).toContain('"message":"auth failed"');
    expect(message).not.toContain("short-secret");
    expect(message).not.toContain("object-secret");
    expect(message).not.toContain("k-9f3a7c2b1e");
    expect(message).not.toContain("opaque-credential");
    expect(message).not.toContain("AIzaRealCredential123");
    expect(message).not.toContain("ghp_abcdef123456");
    expect(message).not.toContain("eyJhbGciOiJIUzI1NiJ9");
    expect(message).not.toContain("sk-ant-api03-multiline-secret");
    expect(message).toContain('"key":"[REDACTED]"');
    expect(message).toContain('"token":"[REDACTED]"');
    expect(message).toContain('"tokens":"[REDACTED]"');
    expect(message).toContain('"auth":"[REDACTED]"');
    expect(message).toContain('"credential":"[REDACTED]"');
    expect(message).toContain(
      '"session":{"activeTab":"providers","scrollPos":120}',
    );
  });

  it("applies property-level redaction to stringified-JSON rejection reasons", () => {
    // JSON in string form can't rely on text regex alone — array/plural/
    // bare-scalar fields would leak through.
    reportFrontendError(
      "unhandledrejection",
      '{"tokens":["k-9f3a7c2b1e"],"auth":["opaque-credential"],"key":"short-secret","keepMe":"visible"}',
    );

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("k-9f3a7c2b1e");
    expect(message).not.toContain("opaque-credential");
    expect(message).not.toContain("short-secret");
    expect(message).toContain('"tokens":"[REDACTED]"');
    expect(message).toContain('"auth":"[REDACTED]"');
    expect(message).toContain('"key":"[REDACTED]"');
    expect(message).toContain('"keepMe":"visible"');
  });

  it("keeps non-JSON error strings as readable text", () => {
    reportFrontendError("window.error", "plain failure at step 3");

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).toContain("plain failure at step 3");
  });

  it("applies property-level redaction to JSON wrapped in an Error message", () => {
    // throw new Error(JSON.stringify(payload)) buries credentials inside
    // message, and the first line of error.stack echoes message verbatim —
    // message must be structurally redacted first.
    reportFrontendError(
      "unhandledrejection",
      new Error(
        '{"tokens":["k-9f3a7c2b1e"],"key":"short-secret","keepMe":"visible"}',
      ),
    );

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("k-9f3a7c2b1e");
    expect(message).not.toContain("short-secret");
    expect(message).toContain('"tokens":"[REDACTED]"');
    expect(message).toContain('"key":"[REDACTED]"');
    expect(message).toContain('"keepMe":"visible"');
  });

  it("omits oversized JSON error strings instead of leaking truncated fields", () => {
    // Valid but oversized JSON that gets truncated before parsing always
    // becomes invalid JSON, falling back to the text tier, and array fields leak.
    const padding = "x".repeat(20_000);
    reportFrontendError(
      "unhandledrejection",
      `{"tokens":["k-9f3a7c2b1e"],"padding":"${padding}"}`,
    );

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("k-9f3a7c2b1e");
    expect(message).toContain("[oversized structured error omitted]");
  });

  it("omits oversized JSON wrapped in an Error message", () => {
    const padding = "x".repeat(20_000);
    reportFrontendError(
      "unhandledrejection",
      new Error(`{"tokens":["k-9f3a7c2b1e"],"padding":"${padding}"}`),
    );

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("k-9f3a7c2b1e");
    expect(message).toContain("[oversized structured error omitted]");
  });

  it("redacts array credentials in prefix+JSON rejection strings", () => {
    // Prefix + JSON: the prefix blocks `redactStructuredString`'s startsWith
    // gate, falling back to the text tier. The text tier's container regex
    // must catch `"tokens":[...]` at this single, unified exit point.
    reportFrontendError(
      "unhandledrejection",
      'Load failed: {"tokens":["ak_live_7f3d9b21c8e4"]}',
    );

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("ak_live_7f3d9b21c8e4");
    expect(message).toContain("Load failed");
  });

  it("redacts array credentials in prefix+JSON wrapped in an Error", () => {
    reportFrontendError(
      "unhandledrejection",
      new Error(
        "Provider provisioning failed: " +
          JSON.stringify({ apiKeys: ["ak_live_7f3d9b21c8e4"] }),
      ),
    );

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("ak_live_7f3d9b21c8e4");
  });

  it("redacts credentials in double-encoded nested JSON (escaped quotes)", () => {
    reportFrontendError(
      "unhandledrejection",
      new Error(
        JSON.stringify({
          status: 400,
          body: '{"keys":["abcd1234efgh5678ij"]}',
        }),
      ),
    );

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("abcd1234efgh5678ij");
  });

  it("redacts array credentials in a POJO (non-Error) rejection", () => {
    reportFrontendError("unhandledrejection", {
      name: "HttpError",
      message: '{"tokens":["opaqueTokenValue12345"]}',
      code: 400,
    });

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("opaqueTokenValue12345");
  });

  it("redacts container values only under sensitive keys", () => {
    // The container regex only fires on sensitive keys: ordinary
    // arrays/objects (items/config) and words that merely end in "key"
    // (monkey) are left untouched.
    const redacted = redactFrontendLogText(
      '{"tokens":["k-secret-1"],"monkey":["visible-a"],"items":["visible-b"],"config":{"theme":"dark"}}',
    );

    expect(redacted).not.toContain("k-secret-1");
    expect(redacted).toContain("visible-a");
    expect(redacted).toContain("visible-b");
    expect(redacted).toContain('"theme":"dark"');
  });

  it("preserves native WebKit-style stack frames for JSON-wrapped errors", () => {
    // macOS/Linux WKWebView uses the `fn@file:line:col` format, and stack
    // does not include message. The old `/^\s+at\s/` filter would drop the
    // entire stack; the new implementation must prepend a redacted message
    // header while preserving the native stack.
    const err = new Error('{"tokens":["k-9f3a7c2b1e"]}');
    Object.defineProperty(err, "stack", {
      value:
        "handleClick@tauri://localhost/assets/index.js:42:15\n" +
        "dispatch@tauri://localhost/assets/index.js:99:3",
    });

    reportFrontendError("unhandledrejection", err);

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("k-9f3a7c2b1e");
    expect(message).toContain('"tokens":"[REDACTED]"'); // redacted message header
    expect(message).toContain("handleClick@tauri://localhost"); // native stack frame preserved
    expect(message).toContain("dispatch@tauri://localhost");
  });

  it("replaces every occurrence of the raw message in a V8-style stack", () => {
    // If message appears multiple times in the stack (eval/anonymous-frame
    // echo), the literal replacement must swap out every occurrence, with
    // zero left behind.
    const err = new Error('{"tokens":["k-9f3a7c2b1e"]}');
    Object.defineProperty(err, "stack", {
      value:
        'Error: {"tokens":["k-9f3a7c2b1e"]}\n' +
        '    at eval (eval at <anonymous>, {"tokens":["k-9f3a7c2b1e"]}:1:1)\n' +
        "    at run (index.js:10:5)",
    });

    reportFrontendError("unhandledrejection", err);

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("k-9f3a7c2b1e");
    expect(message).toContain("at run (index.js:10:5)"); // stack frame preserved
  });

  it("redacts standalone secret shapes in ordinary error text", () => {
    reportFrontendError(
      "window.error",
      new Error("request failed with sk-ant-api03-real-secret"),
    );

    const [message] = writeErrorLog.mock.calls[0];
    expect(message).not.toContain("sk-ant-api03-real-secret");
    expect(message).toContain("[REDACTED]");
  });

  it("handles circular rejection objects", () => {
    const reason: Record<string, unknown> = { message: "failed" };
    reason.self = reason;

    expect(() =>
      reportFrontendError("unhandledrejection", reason),
    ).not.toThrow();
    expect(writeErrorLog.mock.calls[0][0]).toContain("[Circular]");
  });

  it("captures global errors and unhandled rejections and can uninstall", () => {
    const target = new EventTarget() as unknown as Window;
    const uninstall = installGlobalErrorHandlers(target);

    const errorEvent = new Event("error") as ErrorEvent;
    Object.defineProperties(errorEvent, {
      error: { value: new Error("render failed") },
      filename: { value: "app.js" },
      lineno: { value: 10 },
      colno: { value: 4 },
    });
    target.dispatchEvent(errorEvent);

    const rejectionEvent = new Event(
      "unhandledrejection",
    ) as PromiseRejectionEvent;
    Object.defineProperty(rejectionEvent, "reason", {
      value: new Error("request failed"),
    });
    target.dispatchEvent(rejectionEvent);

    expect(writeErrorLog).toHaveBeenCalledTimes(2);

    uninstall();
    target.dispatchEvent(errorEvent);
    expect(writeErrorLog).toHaveBeenCalledTimes(2);
  });
});
