import { useEffect, useMemo, useRef, useState } from "react";
import type { ProviderEditProfile, ToolId } from "@/entities/provider";
import { MAX_PROVIDER_ENDPOINT_CANDIDATES } from "@/native";
import {
  buildProviderEndpointPatch,
  normalizeProviderEndpoint,
  type ProviderEndpointRoute,
  type ProviderEndpointValidation,
} from "./providerEndpointRouteUtils";
import { useProviderEndpointSpeedTest } from "./useProviderEndpointSpeedTest";

export function useProviderEndpointRoutes(
  profile: ProviderEditProfile | null,
  providerId: string | null,
  tool: ToolId | null,
) {
  const sequence = useRef(0);
  const activeProvider = useRef<string | null>(null);
  const profileLoaded = useRef(false);
  const [baseUrl, setBaseUrlState] = useState("");
  const [routes, setRoutes] = useState<ProviderEndpointRoute[]>([]);
  const [autoSelect, setAutoSelectState] = useState(true);
  const [baseUrlDirty, setBaseUrlDirty] = useState(false);
  const [routesDirty, setRoutesDirty] = useState(false);
  const [autoSelectDirty, setAutoSelectDirty] = useState(false);
  const [validation, setValidation] =
    useState<ProviderEndpointValidation | null>(null);

  useEffect(() => {
    if (providerId !== activeProvider.current) {
      activeProvider.current = providerId;
      profileLoaded.current = false;
      setBaseUrlState("");
      setRoutes([]);
      setAutoSelectState(true);
      setBaseUrlDirty(false);
      setRoutesDirty(false);
      setAutoSelectDirty(false);
      setValidation(null);
    }
    if (providerId === null || profile === null || profileLoaded.current)
      return;
    profileLoaded.current = true;
    setBaseUrlState(profile.baseUrl ?? "");
    setRoutes(
      profile.endpointCandidates.map((url, index) => ({
        id: `stored-route-${index}`,
        url,
        stored: true,
      })),
    );
    setAutoSelectState(profile.endpointAutoSelect);
  }, [profile, providerId]);

  const visibleRoutes = useMemo(() => {
    const next = [...routes];
    const selected = normalizeProviderEndpoint(baseUrl);
    if (selected && !next.some((route) => route.url === selected)) {
      next.unshift({ id: "selected-route", url: selected, stored: false });
    }
    return next;
  }, [baseUrl, routes]);
  const preserveSelectedRoute = () => {
    if (baseUrlDirty) return;
    const selected = normalizeProviderEndpoint(baseUrl);
    if (!selected || routes.some((route) => route.url === selected)) return;
    sequence.current += 1;
    setRoutes((current) => [
      ...current,
      {
        id: `previous-route-${sequence.current}`,
        url: selected,
        stored: false,
      },
    ]);
    setRoutesDirty(true);
  };
  const setBaseUrl = (value: string) => {
    preserveSelectedRoute();
    setBaseUrlState(value);
    setBaseUrlDirty(true);
    setValidation(null);
    speedTest.reset();
  };
  const setAutoSelect = (value: boolean) => {
    setAutoSelectState(value);
    setAutoSelectDirty(true);
  };
  const addRoute = (raw: string) => {
    const url = normalizeProviderEndpoint(raw);
    if (!url) return void setValidation("invalid");
    if (visibleRoutes.some((route) => route.url === url)) {
      return void setValidation("duplicate");
    }
    if (visibleRoutes.length >= MAX_PROVIDER_ENDPOINT_CANDIDATES) {
      return void setValidation("limit");
    }
    sequence.current += 1;
    setRoutes((current) => [
      ...current,
      { id: `new-route-${sequence.current}`, url, stored: false },
    ]);
    setRoutesDirty(true);
    setValidation(null);
    speedTest.reset();
    return url;
  };
  const removeRoute = (id: string) => {
    const selected = normalizeProviderEndpoint(baseUrl);
    const route = visibleRoutes.find((candidate) => candidate.id === id);
    if (!route || route.url === selected) return;
    setRoutes((current) => current.filter((candidate) => candidate.id !== id));
    setRoutesDirty(true);
    setValidation(null);
    speedTest.reset();
  };
  const selectRoute = (route: ProviderEndpointRoute) => {
    preserveSelectedRoute();
    speedTest.reset();
    setBaseUrlState(route.url);
    setBaseUrlDirty(true);
    setAutoSelectState(false);
    setAutoSelectDirty(true);
    setValidation(null);
  };
  const speedTest = useProviderEndpointSpeedTest(
    tool,
    visibleRoutes,
    autoSelect,
    (route) => {
      preserveSelectedRoute();
      setBaseUrlState(route.url);
      setBaseUrlDirty(true);
    },
  );
  const runTest = () => {
    if (!speedTest.run()) {
      setValidation("empty");
      return;
    }
    setValidation(null);
  };

  const buildPatch = (includeRoutes = true) => {
    const outcome = buildProviderEndpointPatch({
      baseUrl,
      routes,
      baseUrlDirty,
      routesDirty,
      autoSelectDirty,
      autoSelect,
      includeRoutes,
    });
    if (outcome.validation) setValidation(outcome.validation);
    return outcome.patch;
  };

  return {
    baseUrl,
    routes: visibleRoutes,
    autoSelect,
    measurements: speedTest.measurements,
    testing: speedTest.testing,
    testFailed: speedTest.failed,
    validation,
    dirty: baseUrlDirty || routesDirty || autoSelectDirty,
    setBaseUrl,
    setAutoSelect,
    addRoute,
    removeRoute,
    selectRoute,
    runTest,
    buildPatch,
  };
}
