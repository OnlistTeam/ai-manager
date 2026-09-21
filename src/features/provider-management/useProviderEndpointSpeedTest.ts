import { useEffect, useMemo, useRef, useState } from "react";
import type { ProviderEndpointTestResult, ToolId } from "@/entities/provider";
import { native } from "@/native";
import {
  fastestProviderEndpoint,
  type ProviderEndpointRoute,
} from "./providerEndpointRouteUtils";

export function useProviderEndpointSpeedTest(
  tool: ToolId | null,
  routes: ProviderEndpointRoute[],
  autoSelect: boolean,
  onFastest: (route: ProviderEndpointRoute) => void,
) {
  const generation = useRef(0);
  const [results, setResults] = useState<ProviderEndpointTestResult[]>([]);
  const [testing, setTesting] = useState(false);
  const [failed, setFailed] = useState(false);

  const reset = () => {
    generation.current += 1;
    setResults([]);
    setTesting(false);
    setFailed(false);
  };
  useEffect(() => reset(), [tool]);
  useEffect(() => () => void (generation.current += 1), []);

  const run = () => {
    if (tool === null || routes.length === 0) return false;
    const current = ++generation.current;
    setTesting(true);
    setFailed(false);
    void native.providers
      .testEndpoints(
        tool,
        routes.map(({ id, url }) => ({ id, url })),
      )
      .then(
        (next) => {
          if (generation.current !== current) return;
          setResults(next);
          setTesting(false);
          if (!autoSelect) return;
          const route = fastestProviderEndpoint(next, routes);
          if (route) onFastest(route);
        },
        () => {
          if (generation.current !== current) return;
          setTesting(false);
          setFailed(true);
        },
      );
    return true;
  };

  const measurements = useMemo(
    () => new Map(results.map((result) => [result.candidateId, result])),
    [results],
  );
  return { testing, failed, measurements, reset, run };
}
