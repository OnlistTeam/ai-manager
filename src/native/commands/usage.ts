import { invokeNative } from "../client";
import {
  usageOverviewSchema,
  usageRefreshResultSchema,
  type UsageOverview,
  type UsageRefreshResult,
} from "../schemas/usage";

export const usage = {
  overview(): Promise<UsageOverview> {
    return invokeNative("app_usage_overview", usageOverviewSchema);
  },

  refresh(): Promise<UsageRefreshResult> {
    return invokeNative("app_usage_refresh", usageRefreshResultSchema);
  },
};
