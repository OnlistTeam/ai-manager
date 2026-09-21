import { useQuery, type UseQueryResult } from "@tanstack/react-query";
import { native, type PromptDetail, type ToolId } from "@/native";

export const promptKeys = {
  all: ["prompts"] as const,
  detail: (tool: string, prompt: string) =>
    ["prompts", "detail", tool, prompt] as const,
};

/** Prompt content is fetched only while an explicit editor is open. */
export function usePromptDetail(
  tool: ToolId | null,
  prompt: string | null,
  open: boolean,
): UseQueryResult<PromptDetail, Error> {
  return useQuery({
    queryKey: promptKeys.detail(tool ?? "", prompt ?? ""),
    queryFn: () =>
      tool && prompt
        ? native.prompts.get(tool, prompt)
        : Promise.reject(new Error("Prompt detail scope is incomplete")),
    enabled: open && tool !== null && prompt !== null,
    staleTime: 0,
  });
}
