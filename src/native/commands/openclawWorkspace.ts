import { z } from "zod";
import { invokeNative } from "../client";
import {
  MAX_OPENCLAW_WORKSPACE_CONTENT_BYTES,
  MAX_OPENCLAW_WORKSPACE_SEARCH_CHARS,
  openClawDailyMemoryDocumentSchema,
  openClawDailyMemoryListSchema,
  openClawWorkspaceDirectorySchema,
  openClawWorkspaceDocumentSchema,
  openClawWorkspaceFileIdSchema,
  openClawWorkspaceOverviewSchema,
  openClawWorkspaceWriteOutcomeSchema,
  type OpenClawDailyMemoryDocument,
  type OpenClawDailyMemoryList,
  type OpenClawWorkspaceDirectory,
  type OpenClawWorkspaceDocument,
  type OpenClawWorkspaceFileId,
  type OpenClawWorkspaceOverview,
  type OpenClawWorkspaceWriteOutcome,
} from "../schemas/openclawWorkspace";

const dateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/u);
const querySchema = z.string().trim().max(MAX_OPENCLAW_WORKSPACE_SEARCH_CHARS);
const contentSchema = z.string().superRefine((content, context) => {
  if (
    new TextEncoder().encode(content).byteLength >
    MAX_OPENCLAW_WORKSPACE_CONTENT_BYTES
  ) {
    context.addIssue({
      code: z.ZodIssueCode.custom,
      message: "Workspace content is larger than one MiB",
    });
  }
  if (content.includes("\0")) {
    context.addIssue({
      code: z.ZodIssueCode.custom,
      message: "Workspace content cannot contain NUL bytes",
    });
  }
});

export const openClawWorkspace = {
  overview(): Promise<OpenClawWorkspaceOverview> {
    return invokeNative(
      "app_openclaw_workspace_overview",
      openClawWorkspaceOverviewSchema,
    );
  },

  document(file: OpenClawWorkspaceFileId): Promise<OpenClawWorkspaceDocument> {
    return invokeNative(
      "app_openclaw_workspace_document",
      openClawWorkspaceDocumentSchema,
      { file: openClawWorkspaceFileIdSchema.parse(file) },
    );
  },

  saveDocument(
    file: OpenClawWorkspaceFileId,
    content: string,
  ): Promise<OpenClawWorkspaceWriteOutcome> {
    return invokeNative(
      "app_openclaw_workspace_save_document",
      openClawWorkspaceWriteOutcomeSchema,
      {
        file: openClawWorkspaceFileIdSchema.parse(file),
        content: contentSchema.parse(content),
      },
    );
  },

  memories(query: string): Promise<OpenClawDailyMemoryList> {
    const parsed = querySchema.parse(query);
    return invokeNative(
      "app_openclaw_daily_memories",
      openClawDailyMemoryListSchema,
      { query: parsed || null },
    );
  },

  memory(date: string): Promise<OpenClawDailyMemoryDocument> {
    return invokeNative(
      "app_openclaw_daily_memory",
      openClawDailyMemoryDocumentSchema,
      { date: dateSchema.parse(date) },
    );
  },

  saveMemory(
    date: string,
    content: string,
  ): Promise<OpenClawWorkspaceWriteOutcome> {
    return invokeNative(
      "app_openclaw_daily_memory_save",
      openClawWorkspaceWriteOutcomeSchema,
      {
        date: dateSchema.parse(date),
        content: contentSchema.parse(content),
      },
    );
  },

  deleteMemory(date: string): Promise<OpenClawWorkspaceWriteOutcome> {
    return invokeNative(
      "app_openclaw_daily_memory_delete",
      openClawWorkspaceWriteOutcomeSchema,
      { date: dateSchema.parse(date) },
    );
  },

  openDirectory(directory: OpenClawWorkspaceDirectory): Promise<void> {
    return invokeNative("app_openclaw_workspace_open_directory", z.null(), {
      directory: openClawWorkspaceDirectorySchema.parse(directory),
    }).then(() => undefined);
  },
};
