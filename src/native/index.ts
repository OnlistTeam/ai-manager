import { about } from "./commands/about";
import { backup, backupSchedule } from "./commands/backup";
import { deepLink } from "./commands/deeplink";
import { desktopApps } from "./commands/desktopApps";
import { desktopPreferences } from "./commands/desktopPreferences";
import { extensions } from "./commands/extensions";
import { health } from "./commands/health";
import { importExisting } from "./commands/import";
import { mcp } from "./commands/mcp";
import { networkProxy } from "./commands/networkProxy";
import { openClawWorkspace } from "./commands/openclawWorkspace";
import { operations } from "./commands/operations";
import { prompts } from "./commands/prompts";
import { providers } from "./commands/providers";
import { routing } from "./commands/routing";
import { sessions } from "./commands/sessions";
import { settings } from "./commands/settings";
import { skills } from "./commands/skills";
import { system } from "./commands/system";
import { tools } from "./commands/tools";
import { usage } from "./commands/usage";

export const native = {
  about,
  deepLink,
  desktopApps,
  desktopPreferences,
  tools,
  operations,
  providers,
  routing,
  sessions,
  prompts,
  extensions,
  health,
  importExisting,
  mcp,
  networkProxy,
  openClawWorkspace,
  settings,
  skills,
  backup,
  backupSchedule,
  usage,
  system,
};

export { invokeNative } from "./client";
export {
  desktopAppConfigurationRelationshipSchema,
  desktopAppIdSchema,
  desktopAppInstallerHandoffSchema,
  desktopAppLaunchOutcomeSchema,
  desktopAppListSchema,
  desktopAppOfficialDownloadOutcomeSchema,
  desktopAppSchema,
  desktopAppStatusSchema,
  desktopAppUninstallHandoffSchema,
  desktopAppUninstallOutcomeSchema,
} from "./schemas/desktopApp";
export type {
  DesktopApp,
  DesktopAppConfigurationRelationship,
  DesktopAppId,
  DesktopAppInstallerHandoff,
  DesktopAppLaunchOutcome,
  DesktopAppOfficialDownloadOutcome,
  DesktopAppStatus,
  DesktopAppUninstallHandoff,
  DesktopAppUninstallOutcome,
} from "./schemas/desktopApp";
export {
  deepLinkBlockReasonSchema,
  deepLinkCredentialFieldSchema,
  deepLinkImportOutcomeSchema,
  deepLinkOriginSchema,
  deepLinkPendingEventSchema,
  deepLinkPreviewListSchema,
  deepLinkPreviewSchema,
  deepLinkResourceSchema,
  deepLinkTargetSchema,
} from "./schemas/deeplink";
export type {
  DeepLinkBlockReason,
  DeepLinkCredentialField,
  DeepLinkImportOutcome,
  DeepLinkOrigin,
  DeepLinkPendingEvent,
  DeepLinkPreview,
  DeepLinkResource,
  DeepLinkTarget,
} from "./schemas/deeplink";
export { desktopPreferencesSchema } from "./schemas/desktopPreferences";
export type { DesktopPreferences } from "./schemas/desktopPreferences";
export {
  backupExportOutcomeSchema,
  backupFileSchema,
  backupImportOutcomeSchema,
  backupListSchema,
  backupScheduleSchema,
  restoreOutcomeSchema,
} from "./schemas/backup";
export type {
  BackupExportOutcome,
  BackupFile,
  BackupImportOutcome,
  BackupList,
  BackupSchedule,
  RestoreOutcome,
} from "./schemas/backup";
export {
  importOutcomeSchema,
  importPreviewSchema,
  importSummarySchema,
} from "./schemas/import";
export type {
  ImportOutcome,
  ImportPreview,
  ImportSummary,
} from "./schemas/import";
export {
  configHealthSchema,
  configReadStatusSchema,
  healthProviderTargetSchema,
  healthSnapshotSchema,
  mcpHealthSchema,
  providerHealthSchema,
} from "./schemas/health";
export type {
  ConfigHealth,
  ConfigReadStatus,
  HealthProviderTarget,
  HealthSnapshot,
  McpHealth,
  ProviderHealth,
} from "./schemas/health";
export {
  DEFAULT_PRODUCT_SETTINGS,
  downloadStrategySchema,
  productSettingsSchema,
  terminalAppIdSchema,
} from "./schemas/settings";
export type {
  DownloadStrategy,
  ProductSettings,
  TerminalAppId,
} from "./schemas/settings";
export {
  checkForUpdate,
  getAppUpdateStatus,
  installAppUpdateAndRestart,
  openAppDownloadPage,
  startAppUpdate,
} from "./updater";
export type { AppUpdatePhase, UpdateStatus } from "./updater";
export {
  CONFIG_LOAD_ERROR_EVENT,
  DEEP_LINK_PENDING_EVENT,
  OPERATION_CHANGED_EVENT,
  PROVIDER_CHANGED_EVENT,
  onConfigLoadError,
  onDeepLinkPending,
  onOperationChanged,
  onProviderChanged,
} from "./events";
export type { ProviderChangedPayload } from "./events";
export { initErrorPayloadSchema } from "./schemas/system";
export type { InitErrorPayload } from "./schemas/system";
export {
  NativeError,
  errorCodeSchema,
  nativeErrorPayloadSchema,
} from "./schemas/error";
export type { ErrorCode, NativeErrorPayload } from "./schemas/error";
export { networkProxySettingsSchema } from "./schemas/networkProxy";
export type { NetworkProxySettings } from "./schemas/networkProxy";
export {
  MAX_OPENCLAW_WORKSPACE_CONTENT_BYTES,
  MAX_OPENCLAW_WORKSPACE_SEARCH_CHARS,
  openClawDailyMemoryDocumentSchema,
  openClawDailyMemoryListSchema,
  openClawDailyMemorySummarySchema,
  openClawWorkspaceDirectorySchema,
  openClawWorkspaceDocumentSchema,
  openClawWorkspaceFileIdSchema,
  openClawWorkspaceFileStatusSchema,
  openClawWorkspaceFileSummarySchema,
  openClawWorkspaceOverviewSchema,
  openClawWorkspaceWriteOutcomeSchema,
} from "./schemas/openclawWorkspace";
export type {
  OpenClawDailyMemoryDocument,
  OpenClawDailyMemoryList,
  OpenClawDailyMemorySummary,
  OpenClawWorkspaceDirectory,
  OpenClawWorkspaceDocument,
  OpenClawWorkspaceFileId,
  OpenClawWorkspaceFileStatus,
  OpenClawWorkspaceFileSummary,
  OpenClawWorkspaceOverview,
  OpenClawWorkspaceWriteOutcome,
} from "./schemas/openclawWorkspace";
export {
  detectedSkillResourceActionSchema,
  detectedSkillResourceOpenOutcomeSchema,
  extensionKindSchema,
  extensionListSchema,
  extensionManagementSchema,
  extensionScopeSchema,
  extensionSchema,
  localExtensionInventorySchema,
  localExtensionScopeSchema,
  localExtensionScopeStatusSchema,
} from "./schemas/extension";
export type {
  DetectedSkillResourceAction,
  DetectedSkillResourceOpenOutcome,
  Extension,
  ExtensionKind,
  ExtensionManagement,
  ExtensionScope,
  LocalExtensionInventory,
  LocalExtensionScope,
  LocalExtensionScopeStatus,
} from "./schemas/extension";
export { extensionScopeKey, toolExtensionScope } from "./schemas/extension";
export { mcpConnectionDraftSchema, mcpInstallDraftSchema } from "./schemas/mcp";
export type { McpConnectionDraft, McpInstallDraft } from "./schemas/mcp";
export {
  MAX_PROMPT_CONTENT_BYTES,
  MAX_PROMPT_DESCRIPTION_CHARS,
  MAX_PROMPT_NAME_CHARS,
  promptDetailSchema,
  promptDraftSchema,
} from "./schemas/prompt";
export type { PromptDetail, PromptDraft } from "./schemas/prompt";
export {
  skillBackupListSchema,
  skillBackupSchema,
  skillCatalogItemSchema,
  skillCatalogSchema,
  skillRepositoryDraftSchema,
  skillRepositoryListSchema,
  skillRepositorySchema,
  skillSourceSchema,
  skillUpdateListSchema,
  skillUpdateSchema,
  skillZipInstallOutcomeSchema,
} from "./schemas/skill";
export type {
  SkillBackup,
  SkillCatalogItem,
  SkillRepository,
  SkillRepositoryDraft,
  SkillSource,
  SkillUpdate,
  SkillZipInstallOutcome,
} from "./schemas/skill";
export {
  operationIdSchema,
  operationKindSchema,
  operationExtensionSchema,
  operationLogEntrySchema,
  operationLogKindSchema,
  operationListSchema,
  operationOutputSchema,
  operationSchema,
  operationStatusSchema,
  toolUpdateRecoverySchema,
} from "./schemas/operation";
export type {
  Operation,
  OperationKind,
  OperationExtension,
  OperationLogEntry,
  OperationLogKind,
  OperationOutput,
  OperationStatus,
  ToolUpdateRecovery,
} from "./schemas/operation";
export {
  MAX_PROVIDER_ENDPOINT_CANDIDATES,
  providerConnectionPresetSchema,
  providerConnectionProfileSchema,
  providerCreateDraftSchema,
  providerCreateResultSchema,
  providerCustomCreateDraftSchema,
  providerDraftSchema,
  providerEditCapabilitiesSchema,
  providerEditProfileSchema,
  providerEndpointCandidateListSchema,
  providerEndpointCandidateSchema,
  providerEndpointFailureSchema,
  providerEndpointTestResultListSchema,
  providerEndpointTestResultSchema,
  providerPresetTestResultListSchema,
  providerPreflightOutcomeSchema,
  providerPreflightStatusSchema,
  providerHeaderDraftSchema,
  providerAdvancedDraftSchema,
  providerKindSchema,
  providerListSchema,
  providerReachabilitySchema,
  providerRuntimeContextSchema,
  providerSchema,
  providerTestResultSchema,
} from "./schemas/provider";
export {
  routingOverviewSchema,
  routingProviderSchema,
  routingTargetSchema,
} from "./schemas/routing";
export {
  sessionListSchema,
  sessionMessageRoleSchema,
  sessionMessageSchema,
  sessionReferenceSchema,
  sessionSummarySchema,
  sessionThreadSchema,
} from "./schemas/session";
export type {
  SessionList,
  SessionMessage,
  SessionMessageRole,
  SessionSummary,
  SessionThread,
} from "./schemas/session";
export type {
  RoutingOverview,
  RoutingProvider,
  RoutingTarget,
} from "./schemas/routing";
export type {
  Provider,
  ProviderConnectionPreset,
  ProviderConnectionProfile,
  ProviderCreateDraft,
  ProviderCreateResult,
  ProviderCustomCreateDraft,
  ProviderDraft,
  ProviderAdvancedDraft,
  ProviderEditCapabilities,
  ProviderEditProfile,
  ProviderEndpointCandidate,
  ProviderEndpointFailure,
  ProviderEndpointTestResult,
  ProviderHeaderDraft,
  ProviderKind,
  EffectiveConnection,
  EffectiveConnectionSource,
  EffectiveCredential,
  ProviderPreflightOutcome,
  ProviderPreflightStatus,
  ProviderReachability,
  ProviderRuntimeContext,
  ProviderRuntimeResource,
  ProviderRuntimeResourceAction,
  ProviderRuntimeResourceKind,
  ProviderRuntimeResourceOpenOutcome,
  ProviderRuntimeResourceScope,
  ProviderRuntimeStorage,
  ProviderTestResult,
} from "./schemas/provider";
export {
  toolAccessRequirementSchema,
  toolCapabilitiesSchema,
  toolDiscoverySchema,
  toolIdSchema,
  toolInstallSourceSchema,
  toolLaunchDirectoryModeSchema,
  toolLaunchOutcomeSchema,
  toolListSchema,
  toolSchema,
  toolStatusSchema,
  toolUpdateAttemptPreviewSchema,
  toolUpdateBlockReasonSchema,
  toolUpdateInstallationSchema,
  toolUpdateMethodSchema,
  toolUpdatePreviewListSchema,
  toolUpdatePreviewSchema,
  toolUpdateReadyPreviewSchema,
  updatePreviewFingerprintSchema,
  toolUninstallPreviewSchema,
  toolUninstallTargetKindSchema,
  toolUninstallTargetSchema,
  toolUseCaseSchema,
  toolVersionCatalogSchema,
  toolVersionRestrictionSchema,
  toolVersionTagSchema,
} from "./schemas/tool";
export type {
  Tool,
  ToolAccessRequirement,
  ToolCapabilities,
  ToolDiscovery,
  ToolId,
  ToolInstallSource,
  ToolLaunchDirectoryMode,
  ToolLaunchOutcome,
  ToolStatus,
  ToolUpdateAttemptPreview,
  ToolUpdateBlockReason,
  ToolUpdateInstallation,
  ToolUpdateMethod,
  ToolUpdatePreview,
  ToolUpdateReadyPreview,
  ToolUninstallPreview,
  ToolUninstallTarget,
  ToolUninstallTargetKind,
  ToolUseCase,
  ToolVersionCatalog,
  ToolVersionRestriction,
  ToolVersionTag,
} from "./schemas/tool";
export {
  UNINSTALL_APP_ONLY,
  uninstallOptionsSchema,
} from "./schemas/uninstall";
export type { UninstallOptions } from "./schemas/uninstall";
export {
  usageDaySchema,
  usageMetricsSchema,
  usageOverviewSchema,
  usageRefreshResultSchema,
  usageSyncSummarySchema,
  usageToolBreakdownSchema,
} from "./schemas/usage";
export type {
  UsageDay,
  UsageMetrics,
  UsageOverview,
  UsageRefreshResult,
  UsageSyncSummary,
  UsageToolBreakdown,
} from "./schemas/usage";
