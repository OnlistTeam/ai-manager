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

export const networkProxySettingsSchema = z
  .object({
    configured: z.boolean(),
    url: proxyUrlSchema.nullable(),
    protected: z.boolean(),
  })
  .strict()
  .superRefine((settings, context) => {
    const valid = settings.configured
      ? settings.protected
        ? settings.url === null
        : settings.url !== null
      : settings.url === null && !settings.protected;
    if (!valid) {
      context.addIssue({
        code: "custom",
        message: "network proxy state is inconsistent",
      });
    }
  });

export type NetworkProxySettings = z.infer<typeof networkProxySettingsSchema>;
