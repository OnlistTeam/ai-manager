import type { ToolId } from "../schemas/tool";
import { invokeNative } from "../client";
import {
  routingOverviewSchema,
  type RoutingOverview,
} from "../schemas/routing";
import {
  routingLiveModeOutcomeSchema,
  routingTraceSnapshotSchema,
  type RoutingLiveModeOutcome,
  type RoutingTraceSnapshot,
} from "../schemas/routingTrace";

export const routing = {
  overview(): Promise<RoutingOverview> {
    return invokeNative("app_routing_overview", routingOverviewSchema);
  },

  setTakeover(tool: ToolId, enabled: boolean): Promise<RoutingOverview> {
    return invokeNative("app_routing_set_takeover", routingOverviewSchema, {
      tool,
      enabled,
    });
  },

  setFailover(tool: ToolId, enabled: boolean): Promise<RoutingOverview> {
    return invokeNative("app_routing_set_failover", routingOverviewSchema, {
      tool,
      enabled,
    });
  },

  addToQueue(tool: ToolId, providerId: string): Promise<RoutingOverview> {
    return invokeNative("app_routing_queue_add", routingOverviewSchema, {
      tool,
      providerId,
    });
  },

  removeFromQueue(tool: ToolId, providerId: string): Promise<RoutingOverview> {
    return invokeNative("app_routing_queue_remove", routingOverviewSchema, {
      tool,
      providerId,
    });
  },

  switchProvider(tool: ToolId, providerId: string): Promise<RoutingOverview> {
    return invokeNative("app_routing_switch_provider", routingOverviewSchema, {
      tool,
      providerId,
    });
  },

  stopAll(): Promise<RoutingOverview> {
    return invokeNative("app_routing_stop_all", routingOverviewSchema);
  },

  /** Live routing on or off (ADR-0050). The caller confirms first. */
  setLiveMode(enabled: boolean): Promise<RoutingLiveModeOutcome> {
    return invokeNative(
      "app_routing_set_live_mode",
      routingLiveModeOutcomeSchema,
      { enabled },
    );
  },

  /** The in-memory trace of recent requests, newest first. */
  trace(): Promise<RoutingTraceSnapshot> {
    return invokeNative("app_routing_trace", routingTraceSnapshotSchema);
  },
};
