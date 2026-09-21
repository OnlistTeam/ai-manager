import { useCallback, useEffect, useRef, useState } from "react";
import type { ProviderEndpointTestResult, ToolId } from "@/entities/provider";
import { native } from "@/native";

/** Ephemeral by design: measurements disappear when this renderer process ends. */
export function useProviderPresetSpeedTest(tool: ToolId | null) {
  const generation = useRef(0);
  const [results, setResults] = useState<ProviderEndpointTestResult[]>([]);
  const [measured, setMeasured] = useState(false);
  const [testing, setTesting] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    generation.current += 1;
    setResults([]);
    setMeasured(false);
    setTesting(false);
    setError(null);
  }, [tool]);

  useEffect(
    () => () => {
      generation.current += 1;
    },
    [],
  );

  const run = useCallback(() => {
    if (tool === null) return;
    const current = ++generation.current;
    setTesting(true);
    setError(null);

    void native.providers.testPresets(tool).then(
      (next) => {
        if (generation.current !== current) return;
        setResults(next);
        setMeasured(true);
        setTesting(false);
      },
      (reason: unknown) => {
        if (generation.current !== current) return;
        setError(
          reason instanceof Error ? reason : new Error("Preset test failed"),
        );
        setTesting(false);
      },
    );
  }, [tool]);

  return { results, measured, testing, error, run };
}
