import { useEffect, useRef, useState } from "react";
import type {
  ProviderDraft,
  ProviderEditProfile,
  ToolId,
} from "@/entities/provider";
import { useProviderEndpointRoutes } from "./useProviderEndpointRoutes";

export interface ProviderModelRow {
  id: string;
  value: string;
  existing: boolean;
}

export interface ProviderHeaderRow {
  id: string;
  name: string;
  value: string;
  existing: boolean;
}

export function useProviderSettingsForm(
  profile: ProviderEditProfile | null,
  providerId: string | null,
  tool: ToolId | null,
) {
  const sequence = useRef(0);
  const activeProvider = useRef<string | null>(null);
  const profileLoaded = useRef(false);
  const [models, setModels] = useState<ProviderModelRow[]>([]);
  const [headers, setHeaders] = useState<ProviderHeaderRow[]>([]);
  const [modelsDirty, setModelsDirty] = useState(false);
  const [headersDirty, setHeadersDirty] = useState(false);
  const endpoints = useProviderEndpointRoutes(profile, providerId, tool);

  useEffect(() => {
    if (providerId !== activeProvider.current) {
      activeProvider.current = providerId;
      profileLoaded.current = false;
      setModels([]);
      setHeaders([]);
      setModelsDirty(false);
      setHeadersDirty(false);
    }
    if (providerId === null || profile === null || profileLoaded.current) {
      return;
    }
    profileLoaded.current = true;
    const storedModels = profile.models.map((value, index) => ({
      id: `stored-model-${index}`,
      value,
      existing: true,
    }));
    setModels(
      storedModels.length === 0 &&
        profile.capabilities.canEditModels &&
        !profile.capabilities.supportsMultipleModels
        ? [{ id: "new-single-model", value: "", existing: false }]
        : storedModels,
    );
    setHeaders(
      profile?.headerNames.map((name, index) => ({
        id: `stored-header-${index}`,
        name,
        value: "",
        existing: true,
      })) ?? [],
    );
    setModelsDirty(false);
    setHeadersDirty(false);
  }, [profile, providerId]);

  const addModel = () => {
    setModelsDirty(true);
    sequence.current += 1;
    setModels((current) => [
      ...current,
      {
        id: `new-model-${sequence.current}`,
        value: "",
        existing: false,
      },
    ]);
  };
  const updateModel = (id: string, value: string) => {
    setModelsDirty(true);
    setModels((current) =>
      current.map((row) => (row.id === id ? { ...row, value } : row)),
    );
  };
  const removeModel = (id: string) => {
    setModelsDirty(true);
    setModels((current) => current.filter((row) => row.id !== id));
  };

  const addHeader = () => {
    setHeadersDirty(true);
    sequence.current += 1;
    setHeaders((current) => [
      ...current,
      {
        id: `new-header-${sequence.current}`,
        name: "",
        value: "",
        existing: false,
      },
    ]);
  };
  const updateHeader = (id: string, field: "name" | "value", value: string) => {
    setHeadersDirty(true);
    setHeaders((current) =>
      current.map((row) => (row.id === id ? { ...row, [field]: value } : row)),
    );
  };
  const removeHeader = (id: string) => {
    setHeadersDirty(true);
    setHeaders((current) => current.filter((row) => row.id !== id));
  };
  const multipleModels = profile?.capabilities.supportsMultipleModels ?? false;
  const invalidModels =
    multipleModels && models.some((row) => row.value.trim() === "");
  const invalidHeaders = headers.some(
    (row) =>
      row.name.trim() === "" || (!row.existing && row.value.trim() === ""),
  );

  const buildPatch = (): Pick<ProviderDraft, "models" | "advanced"> | null => {
    if ((modelsDirty && invalidModels) || (headersDirty && invalidHeaders))
      return null;
    if (profile === null) return {};

    const patch: Pick<ProviderDraft, "models" | "advanced"> = {};
    if (profile.capabilities.canEditModels && modelsDirty) {
      patch.models = models
        .map((row) => row.value.trim())
        .filter((value) => value !== "");
    }
    if (
      profile.capabilities.canEditBaseUrl &&
      (endpoints.dirty || headersDirty)
    ) {
      const endpointPatch = endpoints.buildPatch(
        profile.capabilities.canEditEndpoints,
      );
      if (endpointPatch === null) return null;
      patch.advanced = {
        ...endpointPatch,
        headers:
          profile.capabilities.canEditHeaders && headersDirty
            ? headers.map((row) => ({
                name: row.name.trim(),
                value: row.value.trim() === "" ? null : row.value,
              }))
            : null,
      };
    }
    return patch;
  };

  return {
    baseUrl: endpoints.baseUrl,
    models,
    headers,
    invalidModels,
    invalidHeaders,
    setBaseUrl: endpoints.setBaseUrl,
    endpoints,
    addModel,
    updateModel,
    removeModel,
    addHeader,
    updateHeader,
    removeHeader,
    buildPatch,
  };
}
