import { z } from "zod";

const proxyUrlSchema = z
  .string()
  .min(1)
  .max(512)
  .regex(/^(?:https?|socks5h?):\/\//i)
  .refine((value) => {
    try {
      const parsed = new URL(value);
      const loopback =
        parsed.hostname.toLowerCase() === "localhost" ||
        parsed.hostname === "[::1]" ||
        /^127(?:\.\d{1,3}){3}$/.test(parsed.hostname);
      return (
        loopback &&
        parsed.port.length > 0 &&
        parsed.username.length === 0 &&
        parsed.password.length === 0 &&
        (parsed.pathname === "" || parsed.pathname === "/") &&
        parsed.search.length === 0 &&
        parsed.hash.length === 0
      );
    } catch {
      return false;
    }
  });

export const networkProxyModeSchema = z.enum(["auto", "off", "custom"]);
export type NetworkProxyMode = z.infer<typeof networkProxyModeSchema>;

/** The proxy in use, as `scheme://host:port`: the system's may be remote. */
const proxyInUseSchema = z
  .string()
  .min(1)
  .max(512)
  .regex(/^[a-z][a-z0-9+.-]*:\/\/[^/@\s]+$/i);

export const networkProxySettingsSchema = z
  .object({
    mode: networkProxyModeSchema,
    url: proxyUrlSchema.nullable(),
    protected: z.boolean(),
    inUse: proxyInUseSchema.nullable(),
    source: z.enum(["custom", "environment", "system", "none", "off"]),
  })
  .strict()
  .superRefine((settings, context) => {
    const valid =
      settings.mode === "custom"
        ? settings.source === "custom" &&
          (settings.protected
            ? settings.url === null && settings.inUse === null
            : settings.url !== null && settings.inUse === settings.url)
        : settings.url === null &&
          !settings.protected &&
          (settings.mode === "off"
            ? settings.source === "off" && settings.inUse === null
            : settings.source !== "custom" &&
              settings.source !== "off" &&
              (settings.source === "none") === (settings.inUse === null));
    if (!valid) {
      context.addIssue({
        code: "custom",
        message: "network proxy state is inconsistent",
      });
    }
  });

export type NetworkProxySettings = z.infer<typeof networkProxySettingsSchema>;
