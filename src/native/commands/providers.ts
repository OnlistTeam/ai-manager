import { z } from "zod";
import { invokeNative } from "../client";
import {
  providerConnectionProfileSchema,
  providerCreateDraftSchema,
  providerCreateResultSchema,
  providerCustomCreateDraftSchema,
  providerDraftSchema,
  providerEditProfileSchema,
  providerEndpointCandidateListSchema,
  providerEndpointTestResultListSchema,
  providerPresetTestResultListSchema,
  providerListSchema,
  providerPreflightOutcomeSchema,
  providerRuntimeContextSchema,
  providerRuntimeResourceOpenOutcomeSchema,
  providerTestResultSchema,
  type Provider,
  type ProviderConnectionProfile,
  type ProviderCreateDraft,
  type ProviderCreateResult,
  type ProviderCustomCreateDraft,
  type ProviderDraft,
  type ProviderEditProfile,
  type ProviderEndpointCandidate,
  type ProviderEndpointTestResult,
  type ProviderPreflightOutcome,
  type ProviderTestResult,
  type ProviderRuntimeContext,
  type ProviderRuntimeResourceOpenOutcome,
} from "../schemas/provider";
import type { ToolId } from "../schemas/tool";
import { operationIdSchema } from "../schemas/operation";

export const providers = {
  list(tool: ToolId): Promise<Provider[]> {
    return invokeNative("app_providers_list", providerListSchema, { tool });
  },

  runtimeContext(tool: ToolId): Promise<ProviderRuntimeContext> {
    return invokeNative(
      "app_provider_runtime_context",
      providerRuntimeContextSchema,
      { tool },
    );
  },

  openRuntimeResource(
    tool: ToolId,
    resource: string,
  ): Promise<ProviderRuntimeResourceOpenOutcome> {
    return invokeNative(
      "app_provider_runtime_resource_open",
      providerRuntimeResourceOpenOutcomeSchema,
      { tool, resource: z.string().min(1).max(128).parse(resource) },
    );
  },

  connectionProfile(tool: ToolId): Promise<ProviderConnectionProfile> {
    return invokeNative(
      "app_provider_connection_profile",
      providerConnectionProfileSchema,
      { tool },
    );
  },

  editProfile(tool: ToolId, provider: string): Promise<ProviderEditProfile> {
    return invokeNative(
      "app_provider_edit_profile",
      providerEditProfileSchema,
      {
        tool,
        provider,
      },
    );
  },

  create(
    tool: ToolId,
    requestId: string,
    draft: ProviderCreateDraft,
  ): Promise<ProviderCreateResult> {
    return invokeNative("app_provider_create", providerCreateResultSchema, {
      tool,
      requestId: z.string().uuid().parse(requestId),
      draft: providerCreateDraftSchema.parse(draft),
    });
  },

  createCustom(
    tool: ToolId,
    requestId: string,
    draft: ProviderCustomCreateDraft,
  ): Promise<ProviderCreateResult> {
    return invokeNative(
      "app_provider_custom_create",
      providerCreateResultSchema,
      {
        tool,
        requestId: z.string().uuid().parse(requestId),
        draft: providerCustomCreateDraftSchema.parse(draft),
      },
    );
  },

  /** Switching returns the refreshed list along with it, saving a round trip and a flicker. */
  switch(tool: ToolId, provider: string): Promise<Provider[]> {
    return invokeNative("app_provider_switch", providerListSchema, {
      tool,
      provider,
    });
  },

  /** Checks one saved service inside the explicit Use action, then switches if safe. */
  prepareActivation(
    tool: ToolId,
    provider: string,
  ): Promise<ProviderPreflightOutcome> {
    return invokeNative(
      "app_provider_activation_prepare",
      providerPreflightOutcomeSchema,
      { tool, provider },
    );
  },

  /** Checks the active saved service inside the explicit Open action. */
  prepareLaunch(tool: ToolId): Promise<ProviderPreflightOutcome> {
    return invokeNative(
      "app_provider_launch_prepare",
      providerPreflightOutcomeSchema,
      { tool },
    );
  },

  /** Explicitly probes the next saved compatible services and switches once. */
  tryNextHealthy(
    tool: ToolId,
    failedProvider: string,
  ): Promise<ProviderPreflightOutcome> {
    return invokeNative(
      "app_provider_next_healthy",
      providerPreflightOutcomeSchema,
      { tool, failedProvider },
    );
  },

  save(
    tool: ToolId,
    provider: string,
    draft: ProviderDraft,
  ): Promise<Provider[]> {
    return invokeNative("app_provider_save", providerListSchema, {
      tool,
      provider,
      draft: providerDraftSchema.parse(draft),
    });
  },

  remove(tool: ToolId, provider: string): Promise<Provider[]> {
    return invokeNative("app_provider_remove", providerListSchema, {
      tool,
      provider,
    });
  },

  /** Probing takes up to 16 seconds worst case (the backend pins this bound with a test); callers can just render a normal pending state. */
  test(tool: ToolId, provider: string): Promise<ProviderTestResult> {
    return invokeNative("app_provider_test", providerTestResultSchema, {
      tool,
      provider,
    });
  },

  /** Starts one bounded native task for every currently testable saved service. */
  testAll(tool: ToolId): Promise<string> {
    return invokeNative("app_provider_test_all", operationIdSchema, { tool });
  },

  /** Concurrent, credential-free reachability measurements for reviewed candidates. */
  testEndpoints(
    tool: ToolId,
    candidates: ProviderEndpointCandidate[],
  ): Promise<ProviderEndpointTestResult[]> {
    return invokeNative(
      "app_provider_endpoints_test",
      providerEndpointTestResultListSchema,
      {
        tool,
        candidates: providerEndpointCandidateListSchema.parse(candidates),
      },
    );
  },

  /** Tests backend-reviewed presets without exposing their endpoint URLs to the renderer. */
  testPresets(tool: ToolId): Promise<ProviderEndpointTestResult[]> {
    return invokeNative(
      "app_provider_presets_test",
      providerPresetTestResultListSchema,
      { tool },
    );
  },
};
