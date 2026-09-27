import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { z } from "zod";
import {
  deepLinkPendingEventSchema,
  type DeepLinkPendingEvent,
} from "./schemas/deeplink";
import { operationSchema, type Operation } from "./schemas/operation";
import {
  initErrorPayloadSchema,
  type InitErrorPayload,
} from "./schemas/system";
import {
  routingTraceUpdateSchema,
  type RoutingTraceUpdate,
} from "./schemas/routingTrace";
import { routingPickupSchema, type RoutingPickup } from "./schemas/routing";
import { toolIdSchema, type ToolId } from "./schemas/tool";

export const OPERATION_CHANGED_EVENT = "operation://changed";
export const PROVIDER_CHANGED_EVENT = "provider://changed";
export const DEEP_LINK_PENDING_EVENT = "deeplink://pending";
export const CONFIG_LOAD_ERROR_EVENT = "configLoadError";
export const ROUTING_TRACE_EVENT = "routing://trace";
export const QUIT_REQUESTED_EVENT = "app://quit-requested";

export interface ProviderChangedPayload {
  tool: ToolId;
}

export interface RoutedTool {
  tool: ToolId;
  pickup: RoutingPickup;
}

export interface QuitRequestedPayload {
  /** The tools routed through AI Manager when the quit was asked for. */
  tools: RoutedTool[];
}

const quitRequestedPayloadSchema = z
  .object({
    tools: z
      .array(
        z.object({ tool: toolIdSchema, pickup: routingPickupSchema }).strict(),
      )
      .min(1)
      .max(16),
  })
  .strict();

const providerChangedPayloadSchema = z.object({ tool: toolIdSchema }).strict();

/** Subscribes to OperationManager pushes. When the payload doesn't match the schema, discard and log it rather than handing bad data to the UI. */
export function onOperationChanged(
  callback: (operation: Operation) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(OPERATION_CHANGED_EVENT, (event) => {
    const parsed = operationSchema.safeParse(event.payload);
    if (parsed.success) {
      callback(parsed.data);
      return;
    }
    console.error(
      `Discarded malformed ${OPERATION_CHANGED_EVENT} payload`,
      parsed.error.message,
    );
  });
}

/**
 * Pushed when the backend fails to read config. Even when the payload doesn't match
 * the schema, it **still** calls back (with null instead): this is a fatal state the
 * user must be shown, and dropping it would only let the app keep running on broken config.
 */
export function onConfigLoadError(
  callback: (payload: InitErrorPayload | null) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(CONFIG_LOAD_ERROR_EVENT, (event) => {
    const parsed = initErrorPayloadSchema.nullable().safeParse(event.payload);
    if (parsed.success) {
      callback(parsed.data);
      return;
    }
    console.error(
      `Malformed ${CONFIG_LOAD_ERROR_EVENT} payload`,
      parsed.error.message,
    );
    callback(null);
  });
}

/** Tray quick switching only emits the product tool id; provider ids stay native. */
export function onProviderChanged(
  callback: (payload: ProviderChangedPayload) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(PROVIDER_CHANGED_EVENT, (event) => {
    const parsed = providerChangedPayloadSchema.safeParse(event.payload);
    if (parsed.success) {
      callback(parsed.data);
      return;
    }
    console.error(
      `Discarded malformed ${PROVIDER_CHANGED_EVENT} payload`,
      parsed.error.message,
    );
  });
}

/**
 * Pushed when a link arrives through the registered scheme. The payload carries
 * only how many are waiting: the links themselves are read back through the
 * safe preview command, never pushed into the renderer.
 */
export function onDeepLinkPending(
  callback: (payload: DeepLinkPendingEvent) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(DEEP_LINK_PENDING_EVENT, (event) => {
    const parsed = deepLinkPendingEventSchema.safeParse(event.payload);
    if (parsed.success) {
      callback(parsed.data);
      return;
    }
    console.error(
      `Discarded malformed ${DEEP_LINK_PENDING_EVENT} payload`,
      parsed.error.message,
    );
  });
}

/**
 * Pushed for every change to one entry of the live routing trace (ADR-0050).
 * The payload carries only the changed entry and the running totals.
 */
export function onRoutingTrace(
  callback: (update: RoutingTraceUpdate) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(ROUTING_TRACE_EVENT, (event) => {
    const parsed = routingTraceUpdateSchema.safeParse(event.payload);
    if (parsed.success) {
      callback(parsed.data);
      return;
    }
    console.error(
      `Discarded malformed ${ROUTING_TRACE_EVENT} payload`,
      parsed.error.message,
    );
  });
}

/**
 * Pushed when the user quits while tools are routed through AI Manager
 * (ADR-0054). The quit waits for `native.routing.confirmQuit()`.
 */
export function onQuitRequested(
  callback: (payload: QuitRequestedPayload) => void,
): Promise<UnlistenFn> {
  return listen<unknown>(QUIT_REQUESTED_EVENT, (event) => {
    const parsed = quitRequestedPayloadSchema.safeParse(event.payload);
    if (parsed.success) {
      callback(parsed.data);
      return;
    }
    console.error(
      `Discarded malformed ${QUIT_REQUESTED_EVENT} payload`,
      parsed.error.message,
    );
  });
}
