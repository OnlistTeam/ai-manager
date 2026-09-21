import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import { sessionCacheOptions } from "@/lib/query/sessionCache";
import {
  native,
  type OpenClawDailyMemoryDocument,
  type OpenClawDailyMemoryList,
  type OpenClawWorkspaceDirectory,
  type OpenClawWorkspaceDocument,
  type OpenClawWorkspaceFileId,
  type OpenClawWorkspaceOverview,
  type OpenClawWorkspaceWriteOutcome,
} from "@/native";

export const openClawWorkspaceKeys = {
  all: ["openclaw-workspace"] as const,
  overview: ["openclaw-workspace", "overview"] as const,
  documents: ["openclaw-workspace", "document"] as const,
  document: (file: OpenClawWorkspaceFileId) =>
    ["openclaw-workspace", "document", file] as const,
  memories: ["openclaw-workspace", "memories"] as const,
  memoryList: (query: string) =>
    ["openclaw-workspace", "memories", query] as const,
  memoryDocuments: ["openclaw-workspace", "memory"] as const,
  memory: (date: string) => ["openclaw-workspace", "memory", date] as const,
};

export function useOpenClawWorkspaceOverview(): UseQueryResult<
  OpenClawWorkspaceOverview,
  Error
> {
  return useQuery({
    queryKey: openClawWorkspaceKeys.overview,
    queryFn: () => native.openClawWorkspace.overview(),
    ...sessionCacheOptions,
  });
}

export function useOpenClawWorkspaceDocument(
  file: OpenClawWorkspaceFileId | null,
): UseQueryResult<OpenClawWorkspaceDocument, Error> {
  return useQuery({
    queryKey: openClawWorkspaceKeys.document(file ?? "agents"),
    queryFn: () =>
      file === null
        ? Promise.reject(new Error("A workspace file is required"))
        : native.openClawWorkspace.document(file),
    enabled: file !== null,
    ...sessionCacheOptions,
  });
}

export function useOpenClawDailyMemories(
  query: string,
): UseQueryResult<OpenClawDailyMemoryList, Error> {
  return useQuery({
    queryKey: openClawWorkspaceKeys.memoryList(query),
    queryFn: () => native.openClawWorkspace.memories(query),
    ...sessionCacheOptions,
  });
}

export function useOpenClawDailyMemory(
  date: string | null,
): UseQueryResult<OpenClawDailyMemoryDocument, Error> {
  return useQuery({
    queryKey: openClawWorkspaceKeys.memory(date ?? ""),
    queryFn: () =>
      date === null
        ? Promise.reject(new Error("A daily memory date is required"))
        : native.openClawWorkspace.memory(date),
    enabled: date !== null,
    ...sessionCacheOptions,
  });
}

interface SaveDocumentInput {
  file: OpenClawWorkspaceFileId;
  content: string;
}

interface SaveMemoryInput {
  date: string;
  content: string;
}

function invalidateWorkspaceLists(queryClient: QueryClient) {
  return () =>
    Promise.all([
      queryClient.invalidateQueries({
        queryKey: openClawWorkspaceKeys.overview,
      }),
      queryClient.invalidateQueries({
        queryKey: openClawWorkspaceKeys.memories,
      }),
    ]);
}

export function useSaveOpenClawWorkspaceDocument(): UseMutationResult<
  OpenClawWorkspaceWriteOutcome,
  Error,
  SaveDocumentInput
> {
  const queryClient = useQueryClient();
  const invalidate = invalidateWorkspaceLists(queryClient);
  return useMutation({
    mutationFn: ({ file, content }) =>
      native.openClawWorkspace.saveDocument(file, content),
    onSuccess: (_outcome, input) => {
      queryClient.setQueryData<OpenClawWorkspaceDocument>(
        openClawWorkspaceKeys.document(input.file),
        (current) =>
          current
            ? {
                ...current,
                exists: true,
                content: input.content,
                sizeBytes: new TextEncoder().encode(input.content).byteLength,
              }
            : current,
      );
      return invalidate();
    },
  });
}

export function useSaveOpenClawDailyMemory(): UseMutationResult<
  OpenClawWorkspaceWriteOutcome,
  Error,
  SaveMemoryInput
> {
  const queryClient = useQueryClient();
  const invalidate = invalidateWorkspaceLists(queryClient);
  return useMutation({
    mutationFn: ({ date, content }) =>
      native.openClawWorkspace.saveMemory(date, content),
    onSuccess: (_outcome, input) => {
      queryClient.setQueryData<OpenClawDailyMemoryDocument>(
        openClawWorkspaceKeys.memory(input.date),
        (current) =>
          current
            ? {
                ...current,
                exists: true,
                content: input.content,
                sizeBytes: new TextEncoder().encode(input.content).byteLength,
              }
            : current,
      );
      return invalidate();
    },
  });
}

export function useDeleteOpenClawDailyMemory(): UseMutationResult<
  OpenClawWorkspaceWriteOutcome,
  Error,
  string
> {
  const queryClient = useQueryClient();
  const invalidate = invalidateWorkspaceLists(queryClient);
  return useMutation({
    mutationFn: (date) => native.openClawWorkspace.deleteMemory(date),
    onSuccess: (_outcome, date) => {
      queryClient.removeQueries({
        queryKey: openClawWorkspaceKeys.memory(date),
      });
      return invalidate();
    },
  });
}

export function useOpenClawWorkspaceDirectory(): UseMutationResult<
  void,
  Error,
  OpenClawWorkspaceDirectory
> {
  return useMutation({
    mutationFn: (directory) =>
      native.openClawWorkspace.openDirectory(directory),
  });
}
