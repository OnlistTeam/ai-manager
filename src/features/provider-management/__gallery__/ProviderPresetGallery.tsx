import { useEffect, useRef, useState } from "react";
import type {
  ProviderConnectionPreset,
  ProviderEndpointTestResult,
} from "@/entities/provider";
import { ProviderPresetPicker } from "../ProviderPresetPicker";

const PRESETS: ProviderConnectionPreset[] = [
  {
    id: "anthropic",
    serviceName: "Anthropic API",
    defaultName: "Anthropic",
    defaultModel: "claude-sonnet-5",
    baseUrl: "https://api.anthropic.com",
    websiteUrl: "https://www.anthropic.com",
    apiKeyUrl: "https://console.anthropic.com",
    official: true,
  },
  {
    id: "deepseek",
    serviceName: "DeepSeek",
    defaultName: "DeepSeek",
    defaultModel: "deepseek-v4-pro",
    baseUrl: "https://api.example.test",
    websiteUrl: "https://www.deepseek.com",
    apiKeyUrl: "https://platform.deepseek.com",
    official: false,
  },
  {
    id: "openrouter",
    serviceName: "OpenRouter",
    defaultName: "OpenRouter",
    defaultModel: "anthropic/claude-sonnet-4.5",
    baseUrl: "https://api.anthropic.com",
    websiteUrl: "https://openrouter.ai",
    apiKeyUrl: "https://openrouter.ai",
    official: false,
  },
];

const RESULTS: ProviderEndpointTestResult[] = [
  {
    candidateId: "anthropic",
    latencyMs: 318,
    httpStatus: 200,
    failure: null,
  },
  {
    candidateId: "deepseek",
    latencyMs: 84,
    httpStatus: 204,
    failure: null,
  },
  {
    candidateId: "openrouter",
    latencyMs: null,
    httpStatus: null,
    failure: "timeout",
  },
];

/** Dev-only interaction surface for visual QA of the real preset picker. */
export function ProviderPresetGallery() {
  const timer = useRef<number | null>(null);
  const [value, setValue] = useState(PRESETS[0].id);
  const [measurements, setMeasurements] = useState<
    ProviderEndpointTestResult[]
  >([]);
  const [measured, setMeasured] = useState(false);
  const [measuring, setMeasuring] = useState(false);

  useEffect(() => {
    document.documentElement.classList.add("dark");
    return () => {
      if (timer.current !== null) window.clearTimeout(timer.current);
      document.documentElement.classList.remove("dark");
    };
  }, []);

  const measure = () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    setMeasuring(true);
    timer.current = window.setTimeout(() => {
      setMeasurements(RESULTS);
      setMeasured(true);
      setMeasuring(false);
      timer.current = null;
    }, 350);
  };

  return (
    <main className="flex min-h-screen items-center justify-center bg-layer-1 p-8 text-content">
      <section className="w-full max-w-2xl rounded-2xl border border-hairline bg-layer-1 p-6 shadow-lg backdrop-blur-xl">
        <h1 className="text-title">Provider preset speed test</h1>
        <p className="mb-5 mt-1 text-body text-content-muted">
          Development-only interaction surface. No network request is made.
        </p>
        <ProviderPresetPicker
          id="provider-preset-gallery"
          presets={PRESETS}
          value={value}
          measurements={measurements}
          measured={measured}
          measuring={measuring}
          measurementFailed={false}
          onMeasure={measure}
          onChange={setValue}
        />
      </section>
    </main>
  );
}
