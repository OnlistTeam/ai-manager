import { z } from "zod";
import { invokeNative } from "../client";
import {
  shellVariableLocationListSchema,
  shellVariableUpdateSchema,
  shellVariableWrittenSchema,
  type ShellVariableLocation,
  type ShellVariableUpdate,
  type ShellVariableWritten,
} from "../schemas/shellVariable";
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
import {
  modelCatalogSchema,
  modelProbeOutcomeSchema,
  modelProbeRequestSchema,
  type ModelCatalog,
  type ModelProbeOutcome,
  type ModelProbeRequest,
} from "../schemas/modelProbe";

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

  /**
   * Open a preset's "get an API key" page in the system browser.
   *
   * The preset id goes out, never a URL: the address lives in the native
   * catalogue, so the renderer picks a destination rather than naming one.
   */
  openPresetKeyPage(tool: ToolId, preset: string): Promise<null> {
    return invokeNative("app_provider_preset_key_page_open", z.null(), {
      tool,
      preset: z.string().min(1).max(128).parse(preset),
    });
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

  /**
   * Lists the models this saved service actually serves (ADR-0041). A service
   * that refuses answers with a catalogue carrying the refusal, not an error:
   * the dialog still works with a typed model name.
   */
  listModels(tool: ToolId, provider: string): Promise<ModelCatalog> {
    return invokeNative("app_provider_models_list", modelCatalogSchema, {
      tool,
      provider,
    });
  },

  /**
   * Sends one real request to the saved service. This is the only provider
   * probe that spends the user's quota, so it only ever runs from an explicit
   * click. The address and key stay in the backend.
   */
  probeModel(
    tool: ToolId,
    provider: string,
    request: ModelProbeRequest,
  ): Promise<ModelProbeOutcome> {
    return invokeNative("app_provider_model_probe", modelProbeOutcomeSchema, {
      tool,
      provider,
      request: modelProbeRequestSchema.parse(request),
    });
  },

  /**
   * The same two calls for the connection in force when it is not a saved
   * service — one a shell profile exports, or the tool's own configuration file
   * declares. There is no id to send: the backend resolves the connection the
   * next launch will use, exactly as the services page displays it.
   */
  listEffectiveModels(tool: ToolId): Promise<ModelCatalog> {
    return invokeNative(
      "app_provider_effective_models_list",
      modelCatalogSchema,
      { tool },
    );
  },

  probeEffectiveModel(
    tool: ToolId,
    request: ModelProbeRequest,
  ): Promise<ModelProbeOutcome> {
    return invokeNative(
      "app_provider_effective_model_probe",
      modelProbeOutcomeSchema,
      { tool, request: modelProbeRequestSchema.parse(request) },
    );
  },

  /**
   * The start-up lines behind this tool's connection variables (ADR-0042).
   * Only the variables the tool itself reads are searched.
   */
  locateShellVariables(tool: ToolId): Promise<ShellVariableLocation[]> {
    return invokeNative(
      "app_shell_variables_locate",
      shellVariableLocationListSchema,
      { tool },
    );
  },

  /**
   * Replaces one value on one line. `expectedValue` is what the dialog last
   * showed; the backend refuses rather than overwrite a line that changed.
   */
  writeShellVariable(
    tool: ToolId,
    update: ShellVariableUpdate,
  ): Promise<ShellVariableWritten> {
    return invokeNative(
      "app_shell_variable_write",
      shellVariableWrittenSchema,
      { tool, update: shellVariableUpdateSchema.parse(update) },
    );
  },
};
