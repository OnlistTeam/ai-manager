import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";

import { providerKeys } from "@/entities/provider";
import {
  native,
  type ModelCatalog,
  type ModelProbeOutcome,
  type ModelProbeRequest,
  type Provider,
  type ProviderEditProfile,
  type ToolId,
} from "@/native";

/**
 * The model probe's three hooks (ADR-0041).
 *
 * None of them run on mount by themselves: the catalogue only loads once the
 * dialog is open, and the probe itself only ever runs from an explicit click,
 * because it spends the user's quota.
 */

/**
 * What is being tested. A saved service, or the connection in force that has no
 * saved record behind it — one a shell profile exports, or the tool's own
 * configuration file declares. The second kind is just as real to the tool, so
 * it gets the same dialog; it simply has no record to write a model back into.
 */
export type ProbeSubject =
  | { kind: "provider"; provider: Provider }
  | { kind: "effective"; tool: ToolId; name: string };

export function subjectTool(subject: ProbeSubject): ToolId {
  return subject.kind === "provider" ? subject.provider.tool : subject.tool;
}

export function subjectName(subject: ProbeSubject): string {
  return subject.kind === "provider" ? subject.provider.name : subject.name;
}

/** Identity for the effects that reset the dialog when the subject changes. */
export function subjectId(subject: ProbeSubject): string {
  return subject.kind === "provider"
    ? `provider:${subject.provider.tool}:${subject.provider.id}`
    : `effective:${subject.tool}`;
}

const modelCatalogKey = (subject: ProbeSubject | null) =>
  [
    ...providerKeys.all,
    "model-catalog",
    subject === null ? "" : subject.kind,
    subject === null ? "" : subjectTool(subject),
    subject?.kind === "provider" ? subject.provider.id : "",
  ] as const;

/**
 * Reads the service's model list. Not retried: a service that does not publish
 * a catalogue would be asked three times for the same missing page, and the
 * dialog degrades to a typed model name anyway.
 */
export function useModelCatalog(
  subject: ProbeSubject | null,
): UseQueryResult<ModelCatalog, Error> {
  return useQuery({
    queryKey: modelCatalogKey(subject),
    queryFn: () => {
      if (subject === null) {
        return Promise.reject(new Error("A subject is required"));
      }
      return subject.kind === "provider"
        ? native.providers.listModels(
            subject.provider.tool,
            subject.provider.id,
          )
        : native.providers.listEffectiveModels(subject.tool);
    },
    enabled: subject !== null,
    retry: false,
    // A catalogue is worth keeping while the user experiments in the dialog,
    // but not worth carrying into a later session.
    staleTime: 5 * 60_000,
    gcTime: 5 * 60_000,
    refetchOnWindowFocus: false,
  });
}

export interface ModelProbeVariables {
  subject: ProbeSubject;
  request: ModelProbeRequest;
}

/** Sends one real request. Errors surface inline in the dialog, not as a toast. */
export function useProbeModel(): UseMutationResult<
  ModelProbeOutcome,
  Error,
  ModelProbeVariables
> {
  return useMutation<ModelProbeOutcome, Error, ModelProbeVariables>({
    mutationFn: ({ subject, request }) =>
      subject.kind === "provider"
        ? native.providers.probeModel(
            subject.provider.tool,
            subject.provider.id,
            request,
          )
        : native.providers.probeEffectiveModel(subject.tool, request),
  });
}

export interface AdoptModelVariables {
  provider: Provider;
  profile: ProviderEditProfile;
  model: string;
}

/**
 * Writes the tested model back into the service.
 *
 * Only `models` is sent. `apiKey` is null and `advanced` is omitted, so the
 * backend leaves the key and every advanced field exactly as they were — the
 * draft is a patch, not a snapshot of the form.
 */
export function useAdoptModel(): UseMutationResult<
  Provider[],
  Error,
  AdoptModelVariables
> {
  const queryClient = useQueryClient();
  return useMutation<Provider[], Error, AdoptModelVariables>({
    mutationFn: ({ provider, profile, model }) =>
      native.providers.save(provider.tool, provider.id, {
        name: provider.name,
        apiKey: null,
        models: nextModels(profile, model),
      }),
    onSuccess: (providers, { provider }) => {
      queryClient.setQueryData(providerKeys.list(provider.tool), providers);
      void queryClient.invalidateQueries({
        queryKey: providerKeys.editProfile(provider.tool, provider.id),
      });
      void queryClient.invalidateQueries({
        queryKey: providerKeys.runtimeContext(provider.tool),
      });
    },
  });
}

/**
 * A single-model tool replaces its model; a multi-model tool gains one.
 * Appending matches the edit form, where saved model rows are read-only and
 * only new rows can be added.
 */
export function nextModels(
  profile: ProviderEditProfile,
  model: string,
): string[] {
  if (!profile.capabilities.supportsMultipleModels) return [model];
  if (profile.models.includes(model)) return profile.models;
  return [...profile.models, model];
}
