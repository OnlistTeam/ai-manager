import { invokeNative } from "../client";
import {
  networkProxySettingsSchema,
  type NetworkProxySettings,
} from "../schemas/networkProxy";

export const networkProxy = {
  get(): Promise<NetworkProxySettings> {
    return invokeNative("app_network_proxy_get", networkProxySettingsSchema);
  },

  save(url: string | null): Promise<NetworkProxySettings> {
    return invokeNative("app_network_proxy_save", networkProxySettingsSchema, {
      url,
    });
  },
};
