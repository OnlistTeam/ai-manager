import {
  CalendarDays,
  FolderOpen,
  Plus,
  RefreshCw,
  Search,
  Trash2,
} from "lucide-react";
import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  useDeleteOpenClawDailyMemory,
  useOpenClawDailyMemories,
  useOpenClawWorkspaceDirectory,
} from "@/entities/openclaw-workspace";
import { ConfirmActionModal } from "@/features/tool-management";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import { Input } from "@/shared/ui/Input";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import type { WorkspaceEditorTarget } from "./WorkspaceEditorModal";
import { formatWorkspaceBytes, todayMemoryDate } from "./workspaceFormat";

interface DailyMemorySectionProps {
  onEdit: (target: WorkspaceEditorTarget) => void;
}

export function DailyMemorySection({ onEdit }: DailyMemorySectionProps) {
  const { t, i18n } = useTranslation();
  const [input, setInput] = useState("");
  const [query, setQuery] = useState("");
  const [deleting, setDeleting] = useState<string | null>(null);
  const memories = useOpenClawDailyMemories(query);
  const remove = useDeleteOpenClawDailyMemory();
  const openDirectory = useOpenClawWorkspaceDirectory();
  const errorCopy = memories.error ? toErrorCopy(memories.error) : null;

  function search(event: FormEvent): void {
    event.preventDefault();
    setQuery(input.trim());
  }

  function openMemoryDirectory(): void {
    openDirectory.reset();
    openDirectory.mutate("daily-memory", {
      onError: (error) => toast.error(t(toErrorCopy(error).messageKey)),
    });
  }

  function confirmDelete(): void {
    if (!deleting) return;
    remove.reset();
    remove.mutate(deleting, {
      onSuccess: (outcome) => {
        toast.success(t("openClawWorkspace.memories.deleted"), {
          description: outcome.backupCreated
            ? t("openClawWorkspace.memories.deleteBackup")
            : undefined,
        });
        setDeleting(null);
      },
    });
  }

  return (
    <section className="flex min-w-0 flex-col gap-4">
      <SectionHeader
        title={t("openClawWorkspace.memories.title")}
        description={t("openClawWorkspace.memories.description")}
        action={
          <div className="flex flex-wrap justify-end gap-2">
            <Button
              size="sm"
              variant="secondary"
              loading={openDirectory.isPending}
              onClick={openMemoryDirectory}
            >
              <FolderOpen className="h-4 w-4" aria-hidden="true" />
              {t("openClawWorkspace.memories.openFolder")}
            </Button>
            <Button
              size="sm"
              onClick={() =>
                onEdit({ kind: "memory", date: todayMemoryDate() })
              }
            >
              <Plus className="h-4 w-4" aria-hidden="true" />
              {t("openClawWorkspace.memories.today")}
            </Button>
          </div>
        }
      />

      <form className="flex min-w-0 gap-2" onSubmit={search} role="search">
        <Input
          value={input}
          maxLength={200}
          aria-label={t("openClawWorkspace.memories.searchLabel")}
          placeholder={t("openClawWorkspace.memories.searchPlaceholder")}
          onChange={(event) => setInput(event.target.value)}
        />
        <Button type="submit" variant="secondary">
          <Search className="h-4 w-4" aria-hidden="true" />
          {t("openClawWorkspace.memories.search")}
        </Button>
        <Button
          aria-label={t("openClawWorkspace.refresh")}
          variant="ghost"
          loading={memories.isFetching}
          onClick={() => void memories.refetch()}
        >
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
        </Button>
      </form>

      {memories.isPending ? (
        <DetectionStatus label={t("openClawWorkspace.memories.loading")} />
      ) : null}

      {errorCopy ? (
        <Card role="alert" className="border-danger/25 bg-danger/5">
          <p className="text-body font-medium text-content">
            {t(errorCopy.messageKey)}
          </p>
          <Button
            className="mt-3"
            size="sm"
            variant="secondary"
            onClick={() => void memories.refetch()}
          >
            {t("openClawWorkspace.retry")}
          </Button>
        </Card>
      ) : null}

      {memories.data?.items.length === 0 ? (
        <Card padding="none">
          <EmptyState
            icon={CalendarDays}
            title={t(
              query
                ? "openClawWorkspace.memories.emptySearch"
                : "openClawWorkspace.memories.empty",
            )}
            description={t("openClawWorkspace.memories.emptyDescription")}
            action={
              query ? (
                <Button
                  variant="secondary"
                  onClick={() => {
                    setInput("");
                    setQuery("");
                  }}
                >
                  {t("openClawWorkspace.memories.clearSearch")}
                </Button>
              ) : undefined
            }
          />
        </Card>
      ) : null}

      {memories.data && memories.data.items.length > 0 ? (
        <Card
          padding="none"
          className="divide-y divide-line/60 overflow-hidden"
        >
          {memories.data.items.map((memory) => (
            <div
              key={memory.date}
              className="flex min-w-0 items-center gap-2 p-2"
            >
              <button
                type="button"
                className="min-w-0 flex-1 rounded-lg px-3 py-3 text-left transition-colors hover:bg-layer-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                onClick={() => onEdit({ kind: "memory", date: memory.date })}
              >
                <span className="flex flex-wrap items-baseline justify-between gap-2">
                  <span className="text-body font-medium text-content">
                    {memory.date}
                  </span>
                  <span className="text-caption text-content-muted">
                    {formatWorkspaceBytes(memory.sizeBytes, i18n.language)}
                    {memory.matchCount > 0
                      ? ` · ${t("openClawWorkspace.memories.matches", { count: memory.matchCount })}`
                      : ""}
                  </span>
                </span>
                <span className="mt-1 block truncate text-caption text-content-muted">
                  {memory.preview || t("openClawWorkspace.memories.noPreview")}
                </span>
              </button>
              <Button
                size="sm"
                variant="ghost"
                aria-label={t("openClawWorkspace.memories.deleteLabel", {
                  date: memory.date,
                })}
                onClick={() => {
                  remove.reset();
                  setDeleting(memory.date);
                }}
              >
                <Trash2 className="h-4 w-4" aria-hidden="true" />
              </Button>
            </div>
          ))}
          {memories.data.limited ? (
            <p className="px-5 py-3 text-caption text-warning">
              {t("openClawWorkspace.memories.limited")}
            </p>
          ) : null}
        </Card>
      ) : null}

      <ConfirmActionModal
        open={deleting !== null}
        onOpenChange={(open) => {
          if (!open && !remove.isPending) setDeleting(null);
        }}
        title={t("openClawWorkspace.memories.deleteTitle", {
          date: deleting ?? "",
        })}
        description={t("openClawWorkspace.memories.deleteDescription")}
        confirmLabel={t("openClawWorkspace.memories.deleteConfirm")}
        confirmTone="danger"
        busy={remove.isPending}
        error={remove.error}
        onConfirm={confirmDelete}
      />
    </section>
  );
}
