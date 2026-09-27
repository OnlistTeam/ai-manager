import { invokeNative } from "../client";
import {
  networkProxySettingsSchema,
  type NetworkProxyMode,
  type NetworkProxySettings,
} from "../schemas/networkProxy";

export const networkProxy = {
  get(): Promise<NetworkProxySettings> {
    return invokeNative("app_network_proxy_get", networkProxySettingsSchema);
  },

  /** `url` is read only for the custom mode. */
  save(
    mode: NetworkProxyMode,
    url: string | null,
  ): Promise<NetworkProxySettings> {
    return invokeNative("app_network_proxy_save", networkProxySettingsSchema, {
      mode,
      url,
    });
  },
};
