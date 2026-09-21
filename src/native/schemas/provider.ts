import { z } from "zod";
import { toolIdSchema, type ToolId } from "./tool";

export const providerKindSchema = z.enum(["official", "custom"]);

export const providerRuntimeResourceKindSchema = z.enum([
  "configuration",
  "instructions",
  "memory",
  "userProfile",
  "sessionData",
]);

export const providerRuntimeResourceScopeSchema = z.enum(["global", "project"]);

export const providerRuntimeResourceActionSchema = z.enum(["edit", "browse"]);

export const providerRuntimeResourceSchema = z
  .object({
    id: z.string().min(1).max(128),
    kind: providerRuntimeResourceKindSchema,
    scope: providerRuntimeResourceScopeSchema,
    path: z.string().min(1).max(4096),
    exists: z.boolean(),
    action: providerRuntimeResourceActionSchema,
    sizeBytes: z.number().int().nonnegative().nullable(),
    measurementLimited: z.boolean(),
  })
  .strict();

export const providerRuntimeStorageSchema = z
  .object({
    totalBytes: z.number().int().nonnegative(),
    sessionBytes: z.number().int().nonnegative(),
    sessionCount: z.number().int().nonnegative(),
    measurementLimited: z.boolean(),
  })
  .strict();

export const providerRuntimeResourceOpenOutcomeSchema = z.enum([
  "editorOpened",
  "folderOpened",
]);

export const effectiveConnectionSourceSchema = z.discriminatedUnion("kind", [
  z
    .object({
      kind: z.literal("liveConfig"),
      path: z.string().min(1).max(4096),
    })
    .strict(),
  z
    .object({
      kind: z.literal("shellFile"),
      variable: z.string().min(1).max(128),
      path: z.string().min(1).max(4096),
    })
    .strict(),
  z
    .object({
      kind: z.literal("environment"),
      variable: z.string().min(1).max(128),
    })
    .strict(),
  z.object({ kind: z.literal("toolDefault") }).strict(),
]);

export const effectiveCredentialSchema = z.enum([
  "configured",
  "toolLogin",
  "missing",
  "unknown",
]);

/** Aligned field for field with Rust `domain::EffectiveConnection`; has no field that could carry a value. */
export const effectiveConnectionSchema = z
  .object({
    selection: z
      .enum(["configuration", "defaultModel", "recentModel", "unknown"])
      .optional(),
    model: z.string().max(256).nullable().optional(),
    endpoint: z.string().max(2_048).nullable(),
    endpointSource: effectiveConnectionSourceSchema,
    credential: effectiveCredentialSchema,
    credentialSource: effectiveConnectionSourceSchema,
    providerId: z.string().min(1).max(128).nullable(),
    shellInspected: z.boolean(),
  })
  .strict();

export const providerRuntimeContextSchema = z
  .object({
    tool: toolIdSchema,
    liveConfigPaths: z.array(z.string().min(1).max(4096)).max(4),
    resources: z.array(providerRuntimeResourceSchema).max(16),
    storage: providerRuntimeStorageSchema,
    effectiveConnection: effectiveConnectionSchema.nullable(),
  })
  .strict();

/**
 * Aligned field for field with Rust's `domain::provider::Provider`.
 * `apiKey` is the raw secret: in a local desktop app, the raw string is exactly what
 * the user needs to verify, copy, and edit -- masking only protects the user from
 * themselves. Logs and error messages must still never include it (AI_RULES rule 6).
 */
export const providerSchema = z
  .object({
    additive: z.boolean().optional(),
    id: z.string(),
    tool: toolIdSchema,
    name: z.string(),
    kind: providerKindSchema,
    active: z.boolean(),
    baseUrl: z.string().nullable(),
    apiKey: z.string().nullable(),
    websiteUrl: z.string().nullable(),
    testable: z.boolean(),
    canRemove: z.boolean(),
  })
  .strict();

export const providerListSchema = z.array(providerSchema);

export const providerConnectionPresetSchema = z
  .object({
    id: z.string().min(1).max(64),
    serviceName: z.string().min(1).max(100),
    defaultName: z.string().min(1).max(100),
    defaultModel: z.string().min(1).max(256),
    websiteUrl: z.string().url(),
    apiKeyUrl: z.string().url(),
    official: z.boolean(),
  })
  .strict();

export const providerConnectionProfileSchema = z
  .object({
    defaultPresetId: z.string().min(1).max(64),
    modelRequired: z.boolean(),
    presets: z.array(providerConnectionPresetSchema).min(1).max(256),
  })
  .strict()
  .refine(
    (profile) =>
      profile.presets.some((preset) => preset.id === profile.defaultPresetId),
    { message: "Default provider preset is missing" },
  );

export const MAX_PROVIDER_ENDPOINT_CANDIDATES = 32;

const providerEndpointUrlSchema = z
  .string()
  .min(1)
  .max(2_048)
  .refine((value) => value.trim() === value, "Endpoint URL must be trimmed")
  .refine((value) => {
    try {
      const parsed = new URL(value);
      return (
        (parsed.protocol === "http:" || parsed.protocol === "https:") &&
        parsed.hostname.length > 0 &&
        parsed.username.length === 0 &&
        parsed.password.length === 0 &&
        parsed.search.length === 0 &&
        parsed.hash.length === 0
      );
    } catch {
      return false;
    }
  }, "Endpoint URL must be a credential-free HTTP(S) base URL");

export const providerEditCapabilitiesSchema = z
  .object({
    canEditBaseUrl: z.boolean(),
    canEditEndpoints: z.boolean(),
    canEditModels: z.boolean(),
    canEditHeaders: z.boolean(),
    supportsMultipleModels: z.boolean(),
  })
  .strict();

/** Safe edit detail: header values and raw provider configuration never come back. */
export const providerEditProfileSchema = z
  .object({
    providerId: z.string(),
    baseUrl: z.string().nullable(),
    endpointCandidates: z
      .array(providerEndpointUrlSchema)
      .max(MAX_PROVIDER_ENDPOINT_CANDIDATES),
    endpointAutoSelect: z.boolean(),
    models: z.array(z.string()),
    headerNames: z.array(z.string()),
    capabilities: providerEditCapabilitiesSchema,
  })
  .strict()
  .refine(
    (profile) =>
      new Set(profile.endpointCandidates).size ===
      profile.endpointCandidates.length,
    { message: "Endpoint candidates must be unique" },
  );

export const providerReachabilitySchema = z.enum([
  "operational",
  "degraded",
  "failed",
]);

export const providerTestResultSchema = z
  .object({
    providerId: z.string(),
    reachability: providerReachabilitySchema,
    responseTimeMs: z.number().int().nullable(),
    httpStatus: z.number().int().nullable(),
  })
  .strict();

export const providerPreflightStatusSchema = z.enum([
  "ready",
  "unreachable",
  "failedOver",
  "notChecked",
]);

/**
 * A user-action-bound preflight result. Saved provider rows and safe measurements
 * may cross IPC; endpoint candidates and credentials must not.
 */
export const providerPreflightOutcomeSchema = z
  .object({
    status: providerPreflightStatusSchema,
    originProviderId: z.string().nullable(),
    activeProviderId: z.string().nullable(),
    providers: providerListSchema,
    checks: z.array(providerTestResultSchema).max(512),
  })
  .strict()
  .superRefine((outcome, context) => {
    const providerIds = new Set(
      outcome.providers.map((provider) => provider.id),
    );
    const activeIds = outcome.providers
      .filter((provider) => provider.active)
      .map((provider) => provider.id);

    if (
      outcome.originProviderId !== null &&
      !providerIds.has(outcome.originProviderId)
    ) {
      context.addIssue({
        code: "custom",
        path: ["originProviderId"],
        message:
          "Origin provider must be present in the returned provider list",
      });
    }
    if (
      outcome.activeProviderId !== null &&
      (!providerIds.has(outcome.activeProviderId) ||
        !activeIds.includes(outcome.activeProviderId))
    ) {
      context.addIssue({
        code: "custom",
        path: ["activeProviderId"],
        message: "Active provider must match an active returned provider",
      });
    }
    if (
      (activeIds.length === 0 && outcome.activeProviderId !== null) ||
      (activeIds.length > 0 &&
        (activeIds.length !== 1 || outcome.activeProviderId !== activeIds[0]))
    ) {
      context.addIssue({
        code: "custom",
        path: ["providers"],
        message: "Returned providers must agree on exactly one active provider",
      });
    }
    if (outcome.checks.some((check) => !providerIds.has(check.providerId))) {
      context.addIssue({
        code: "custom",
        path: ["checks"],
        message: "Every check must reference a returned provider",
      });
    }

    const healthyChecks = outcome.checks.filter(
      (check) => check.reachability !== "failed",
    );
    if (outcome.status === "notChecked" && outcome.checks.length !== 0) {
      context.addIssue({
        code: "custom",
        path: ["checks"],
        message: "A not-checked result cannot contain measurements",
      });
    }
    if (outcome.status === "ready" && healthyChecks.length === 0) {
      context.addIssue({
        code: "custom",
        path: ["checks"],
        message: "A ready result requires a healthy measurement",
      });
    }
    if (outcome.status === "unreachable" && healthyChecks.length !== 0) {
      context.addIssue({
        code: "custom",
        path: ["checks"],
        message: "An unreachable result cannot contain a healthy measurement",
      });
    }
    if (
      outcome.status === "failedOver" &&
      (outcome.activeProviderId === null ||
        !healthyChecks.some(
          (check) => check.providerId === outcome.activeProviderId,
        ))
    ) {
      context.addIssue({
        code: "custom",
        path: ["checks"],
        message: "A failover result requires a healthy active provider",
      });
    }
  });

/** Input-only endpoint. The backend correlates results by `id` and never echoes the URL. */
export const providerEndpointCandidateSchema = z
  .object({
    id: z
      .string()
      .min(1)
      .max(128)
      .refine(
        (value) =>
          // eslint-disable-next-line no-control-regex -- control characters are the validation target
          value.trim() === value && !/[\u0000-\u001f\u007f]/u.test(value),
      ),
    url: providerEndpointUrlSchema,
  })
  .strict();

export const providerEndpointCandidateListSchema = z
  .array(providerEndpointCandidateSchema)
  .max(MAX_PROVIDER_ENDPOINT_CANDIDATES)
  .refine(
    (candidates) =>
      new Set(candidates.map((candidate) => candidate.id)).size ===
      candidates.length,
    "Endpoint candidate IDs must be unique",
  );

export const providerEndpointFailureSchema = z.enum([
  "invalidUrl",
  "timeout",
  "dns",
  "tls",
  "connection",
  "request",
]);

export const providerEndpointTestResultSchema = z
  .object({
    candidateId: z.string().min(1).max(128),
    latencyMs: z.number().int().nonnegative().nullable(),
    httpStatus: z.number().int().min(100).max(999).nullable(),
    failure: providerEndpointFailureSchema.nullable(),
  })
  .strict()
  .superRefine((result, context) => {
    const success = result.failure === null;
    const hasMeasurement =
      result.latencyMs !== null && result.httpStatus !== null;
    if (success !== hasMeasurement) {
      context.addIssue({
        code: "custom",
        message: "Endpoint results must be either a measurement or a failure",
      });
    }
  });

export const providerEndpointTestResultListSchema = z
  .array(providerEndpointTestResultSchema)
  .max(MAX_PROVIDER_ENDPOINT_CANDIDATES);

export const providerPresetTestResultListSchema = z
  .array(providerEndpointTestResultSchema)
  .max(512);

/**
 * Edit form payload. The secret only ever flows UI -> backend; `apiKey: null` means
 * "leave the stored secret unchanged", so renaming a provider never forces the user
 * to re-enter the secret.
 */
export const providerHeaderDraftSchema = z
  .object({
    name: z.string(),
    value: z.string().nullable(),
  })
  .strict();

export const providerAdvancedDraftSchema = z
  .object({
    baseUrlChanged: z.boolean(),
    baseUrl: z.string().nullable(),
    endpointCandidates: z
      .array(providerEndpointUrlSchema)
      .max(MAX_PROVIDER_ENDPOINT_CANDIDATES)
      .refine(
        (candidates) => new Set(candidates).size === candidates.length,
        "Endpoint candidates must be unique",
      )
      .nullable(),
    endpointAutoSelect: z.boolean().nullable(),
    headers: z.array(providerHeaderDraftSchema).nullable(),
  })
  .strict();

export const providerDraftSchema = z
  .object({
    name: z.string(),
    apiKey: z.string().nullable(),
    models: z.array(z.string()).optional(),
    advanced: providerAdvancedDraftSchema.optional(),
  })
  .strict();

/** Beginner Mode creation payload; the endpoint and internal config shape are owned by the backend template. */
export const providerCreateDraftSchema = z
  .object({
    presetId: z.string().min(1).max(64),
    name: z.string(),
    apiKey: z.string(),
    model: z.string(),
  })
  .strict();

const providerCustomBaseUrlSchema = z
  .string()
  .min(1)
  .max(2_048)
  .refine((value) => value.trim() === value, "Base URL must be trimmed")
  .refine((value) => {
    try {
      const parsed = new URL(value);
      return (
        parsed.protocol === "https:" &&
        parsed.hostname.length > 0 &&
        parsed.username.length === 0 &&
        parsed.password.length === 0 &&
        !value.includes("?") &&
        !value.includes("#") &&
        parsed.search.length === 0 &&
        parsed.hash.length === 0
      );
    } catch {
      return false;
    }
  }, "Custom Base URL must be credential-free HTTPS");

/** Advanced-only custom service payload; native still owns every tool shape. */
export const providerCustomCreateDraftSchema = z
  .object({
    name: z.string().trim().min(1).max(100),
    apiKey: z
      .string()
      .trim()
      .min(1)
      .max(32 * 1_024),
    model: z.string().trim().max(256),
    baseUrl: providerCustomBaseUrlSchema,
  })
  .strict();

/** Safe create response used to target the exact new service for a follow-up check. */
export const providerCreateResultSchema = z
  .object({
    providers: providerListSchema,
    createdProviderId: z.string(),
  })
  .strict();

export type ProviderKind = z.infer<typeof providerKindSchema>;
export type EffectiveConnectionSource = z.infer<
  typeof effectiveConnectionSourceSchema
>;
export type EffectiveCredential = z.infer<typeof effectiveCredentialSchema>;
export type EffectiveConnection = z.infer<typeof effectiveConnectionSchema>;
export type ProviderRuntimeResourceKind = z.infer<
  typeof providerRuntimeResourceKindSchema
>;
export type ProviderRuntimeResourceScope = z.infer<
  typeof providerRuntimeResourceScopeSchema
>;
export type ProviderRuntimeResourceAction = z.infer<
  typeof providerRuntimeResourceActionSchema
>;
export type ProviderRuntimeResource = z.infer<
  typeof providerRuntimeResourceSchema
>;
export type ProviderRuntimeStorage = z.infer<
  typeof providerRuntimeStorageSchema
>;
export type ProviderRuntimeResourceOpenOutcome = z.infer<
  typeof providerRuntimeResourceOpenOutcomeSchema
>;
export type ProviderRuntimeContext = z.infer<
  typeof providerRuntimeContextSchema
>;
export type Provider = z.infer<typeof providerSchema>;
export type ProviderConnectionProfile = z.infer<
  typeof providerConnectionProfileSchema
>;
export type ProviderConnectionPreset = z.infer<
  typeof providerConnectionPresetSchema
>;
export type ProviderEditCapabilities = z.infer<
  typeof providerEditCapabilitiesSchema
>;
export type ProviderEditProfile = z.infer<typeof providerEditProfileSchema>;
export type ProviderHeaderDraft = z.infer<typeof providerHeaderDraftSchema>;
export type ProviderAdvancedDraft = z.infer<typeof providerAdvancedDraftSchema>;
export type ProviderCreateDraft = z.infer<typeof providerCreateDraftSchema>;
export type ProviderCustomCreateDraft = z.infer<
  typeof providerCustomCreateDraftSchema
>;
export type ProviderCreateResult = z.infer<typeof providerCreateResultSchema>;
export type ProviderReachability = z.infer<typeof providerReachabilitySchema>;
export type ProviderTestResult = z.infer<typeof providerTestResultSchema>;
export type ProviderPreflightStatus = z.infer<
  typeof providerPreflightStatusSchema
>;
export type ProviderPreflightOutcome = z.infer<
  typeof providerPreflightOutcomeSchema
>;
export type ProviderEndpointCandidate = z.infer<
  typeof providerEndpointCandidateSchema
>;
export type ProviderEndpointFailure = z.infer<
  typeof providerEndpointFailureSchema
>;
export type ProviderEndpointTestResult = z.infer<
  typeof providerEndpointTestResultSchema
>;
export type ProviderDraft = z.infer<typeof providerDraftSchema>;
export type { ToolId };
