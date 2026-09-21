import { invokeNative } from "../client";
import { healthSnapshotSchema, type HealthSnapshot } from "../schemas/health";
import type { ToolId } from "../schemas/tool";

export const health = {
  snapshot(tools: ToolId[]): Promise<HealthSnapshot> {
    return invokeNative("app_health_snapshot", healthSnapshotSchema, { tools });
  },
};
