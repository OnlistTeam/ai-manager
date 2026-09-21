import { z } from "zod";
import { invokeNative } from "../client";
import {
  sessionListSchema,
  sessionReferenceSchema,
  sessionThreadSchema,
  type SessionList,
  type SessionThread,
} from "../schemas/session";
import { toolIdSchema, type ToolId } from "../schemas/tool";

const querySchema = z.string().trim().max(200);

export const sessions = {
  list(query: string, tool: ToolId | null): Promise<SessionList> {
    return invokeNative("app_sessions_list", sessionListSchema, {
      query: querySchema.parse(query) || null,
      tool: tool === null ? null : toolIdSchema.parse(tool),
    });
  },

  thread(reference: string): Promise<SessionThread> {
    return invokeNative("app_session_thread", sessionThreadSchema, {
      reference: sessionReferenceSchema.parse(reference),
    });
  },

  resume(reference: string): Promise<void> {
    return invokeNative("app_session_resume", z.null(), {
      reference: sessionReferenceSchema.parse(reference),
    }).then(() => undefined);
  },

  revealFolder(reference: string): Promise<void> {
    return invokeNative("app_session_reveal", z.null(), {
      reference: sessionReferenceSchema.parse(reference),
    }).then(() => undefined);
  },
};
