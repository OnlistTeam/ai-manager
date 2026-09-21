import { Toaster as DesignSystemToaster } from "@/shared/ui/Toaster";

/**
 * Adapter kept at the old path so main.tsx and every existing import keep
 * working. The product has a single coloured appearance (tokens.css), so the
 * host is pinned to sonner's dark palette rather than reading an app theme.
 */
export function Toaster() {
  return <DesignSystemToaster theme="dark" />;
}
