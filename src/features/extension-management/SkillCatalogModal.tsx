import {
  Archive,
  ArchiveRestore,
  LoaderCircle,
  Search,
  Settings2,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { SkillCatalogItem } from "@/entities/skill-catalog";
import type { ToolId } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { Input } from "@/shared/ui/Input";
import { Modal } from "@/shared/ui/Modal";
import { ExtensionMutationPausedNotice } from "./ExtensionMutationPausedNotice";
import { SkillCatalogInventoryNotice } from "./SkillCatalogInventoryNotice";
import { SkillCatalogItemRow } from "./SkillCatalogItemRow";
import { SkillCatalogMirrorNotice } from "./SkillCatalogMirrorNotice";
import { SkillBackupsPanel } from "./SkillBackupsPanel";
import { SkillRepositoriesPanel } from "./SkillRepositoriesPanel";
import { SkillZipInstallPanel } from "./SkillZipInstallPanel";
import { useSkillCatalogRecovery } from "./useSkillCatalogRecovery";
import { useInstallSkill } from "./useSkillInstallation";
import { useInstallSkillZip } from "./useSkillZipInstallation";

export interface SkillCatalogModalProps {
  open: boolean;
  tool: ToolId;
  toolName: string;
  mutationsBlocked: boolean;
  onOpenChange: (open: boolean) => void;
}

function matches(skill: SkillCatalogItem, query: string): boolean {
  const normalized = query.trim().toLocaleLowerCase();
  if (normalized === "") return true;
  return [
    skill.name,
    skill.description ?? "",
    skill.source.owner,
    skill.source.repository,
  ].some((value) => value.toLocaleLowerCase().includes(normalized));
}

export function SkillCatalogModal({
  open,
  tool,
  toolName,
  mutationsBlocked,
  onOpenChange,
}: SkillCatalogModalProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [view, setView] = useState<
    "catalog" | "repositories" | "zip" | "backups"
  >("catalog");
  const [backupMutationPending, setBackupMutationPending] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const {
    catalog,
    retryButtonRef,
    catalogAvailable,
    catalogInitiallyLoading,
    catalogUnavailable,
    catalogRefreshFailed,
    catalogRefreshingRetained,
    catalogActionsBlocked,
    retryCatalog,
  } = useSkillCatalogRecovery(tool, open, searchRef);
  const install = useInstallSkill();
  const installZip = useInstallSkillZip();
  const resetInstall = install.reset;
  const resetInstallZip = installZip.reset;
  const visible = useMemo(
    () => (catalog.data ?? []).filter((skill) => matches(skill, query)),
    [catalog.data, query],
  );
  const mirrorUsed = useMemo(
    () => (catalog.data ?? []).some((skill) => skill.mirrorUsed),
    [catalog.data],
  );

  useEffect(() => {
    if (!open) {
      setQuery("");
      setView("catalog");
      setBackupMutationPending(false);
    }
    resetInstall();
    resetInstallZip();
  }, [open, resetInstall, resetInstallZip, tool]);

  const busy =
    install.isPending || installZip.isPending || backupMutationPending;

  return (
    <Modal
      open={open}
      size="lg"
      dismissible={!busy}
      initialFocusRef={searchRef}
      onOpenChange={onOpenChange}
      title={t(
        view === "repositories"
          ? "extensions.skill.repositories.title"
          : view === "zip"
            ? "extensions.skill.zip.title"
            : view === "backups"
              ? "extensions.skill.backups.title"
              : "extensions.skill.catalog.title",
      )}
      description={
        view === "repositories"
          ? t("extensions.skill.repositories.description")
          : view === "zip"
            ? t("extensions.skill.zip.description", { tool: toolName })
            : view === "backups"
              ? t("extensions.skill.backups.description", { tool: toolName })
              : t("extensions.skill.catalog.description", { tool: toolName })
      }
      footer={
        <>
          {view !== "catalog" ? (
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => {
                setView("catalog");
                window.requestAnimationFrame(() => searchRef.current?.focus());
              }}
            >
              {t("extensions.skill.repositories.back")}
            </Button>
          ) : null}
          <Button
            variant="ghost"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t("ds.action.close")}
          </Button>
        </>
      }
    >
      {mutationsBlocked ? <ExtensionMutationPausedNotice /> : null}

      {view === "repositories" ? (
        <SkillRepositoriesPanel />
      ) : view === "backups" ? (
        <SkillBackupsPanel
          tool={tool}
          blocked={mutationsBlocked}
          onBusyChange={setBackupMutationPending}
          onRestoreStarted={() => onOpenChange(false)}
        />
      ) : view === "zip" ? (
        <SkillZipInstallPanel
          blocked={mutationsBlocked}
          pending={installZip.isPending}
          error={installZip.error}
          onChoose={() => {
            if (mutationsBlocked || installZip.isPending) return;
            installZip.reset();
            installZip.mutate(tool, {
              onSuccess: (outcome) => {
                if (outcome.status === "started") onOpenChange(false);
              },
            });
          }}
        />
      ) : (
        <>
          <label className="relative block">
            <span className="sr-only">
              {t("extensions.skill.catalog.searchLabel")}
            </span>
            <Search
              className="pointer-events-none absolute left-3 top-3 h-4 w-4 text-content-muted"
              aria-hidden="true"
            />
            <Input
              ref={searchRef}
              type="search"
              value={query}
              className="pl-9"
              placeholder={t("extensions.skill.catalog.searchPlaceholder")}
              onChange={(event) => setQuery(event.target.value)}
            />
          </label>

          <aside className="mt-3 flex flex-wrap items-center gap-2 rounded-lg bg-layer-1 px-3 py-2.5 text-caption text-content-muted">
            <ShieldCheck
              className="h-4 w-4 shrink-0 text-brand"
              aria-hidden="true"
            />
            <p className="min-w-0 flex-1">
              {t("extensions.skill.catalog.trustNote")}
            </p>
            <div className="flex flex-wrap items-center justify-end gap-1">
              <Button
                size="sm"
                variant="ghost"
                disabled={mutationsBlocked || busy}
                onClick={() => {
                  installZip.reset();
                  setView("zip");
                }}
              >
                <Archive className="h-4 w-4" aria-hidden="true" />
                {t("extensions.skill.zip.action")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={busy}
                onClick={() => setView("backups")}
              >
                <ArchiveRestore className="h-4 w-4" aria-hidden="true" />
                {t("extensions.skill.backups.action")}
              </Button>
              <Button
                size="sm"
                variant="ghost"
                disabled={busy}
                onClick={() => setView("repositories")}
              >
                <Settings2 className="h-4 w-4" aria-hidden="true" />
                {t("extensions.skill.repositories.manage")}
              </Button>
            </div>
          </aside>

          {catalogInitiallyLoading ? (
            <div
              role="status"
              className="flex min-h-64 flex-col items-center justify-center gap-3 text-content-muted"
            >
              <LoaderCircle
                className="h-6 w-6 text-brand motion-safe:animate-spin"
                aria-hidden="true"
              />
              <p>{t("extensions.skill.catalog.loading")}</p>
            </div>
          ) : null}

          {catalogUnavailable ? (
            <SkillCatalogInventoryNotice
              mode="unavailable"
              refreshing={catalog.isFetching}
              retryButtonRef={retryButtonRef}
              onRetry={retryCatalog}
            />
          ) : null}

          {catalogRefreshFailed ? (
            <SkillCatalogInventoryNotice
              mode="retained"
              refreshing={catalog.isFetching}
              retryButtonRef={retryButtonRef}
              onRetry={retryCatalog}
            />
          ) : null}

          {catalogRefreshingRetained ? (
            <SkillCatalogInventoryNotice
              mode="refreshing"
              refreshing
              onRetry={retryCatalog}
            />
          ) : null}

          {catalogAvailable && mirrorUsed ? <SkillCatalogMirrorNotice /> : null}

          {catalogAvailable && visible.length === 0 ? (
            <div className="flex min-h-64 flex-col items-center justify-center px-6 text-center">
              <Sparkles className="h-8 w-8 text-brand" aria-hidden="true" />
              <p className="mt-3 font-medium text-content">
                {t(
                  query.trim()
                    ? "extensions.skill.catalog.noResults"
                    : "extensions.skill.catalog.empty",
                )}
              </p>
              <p className="mt-1 max-w-md text-caption text-content-muted">
                {t(
                  query.trim()
                    ? "extensions.skill.catalog.noResultsDescription"
                    : "extensions.skill.catalog.emptyDescription",
                )}
              </p>
            </div>
          ) : null}

          {catalogAvailable && visible.length > 0 ? (
            <ul className="scrollbar-subtle mt-4 max-h-[52vh] space-y-2 overflow-y-auto pr-1">
              {visible.map((skill) => {
                const pending =
                  install.isPending && install.variables?.skill.id === skill.id;
                return (
                  <SkillCatalogItemRow
                    key={skill.id}
                    skill={skill}
                    busy={
                      install.isPending ||
                      mutationsBlocked ||
                      catalogActionsBlocked
                    }
                    pending={pending}
                    error={
                      install.isError &&
                      install.variables?.skill.id === skill.id
                        ? install.error
                        : null
                    }
                    onInstall={(picked) => {
                      if (mutationsBlocked || catalogActionsBlocked) return;
                      install.mutate(
                        { tool, skill: picked },
                        { onSuccess: () => onOpenChange(false) },
                      );
                    }}
                  />
                );
              })}
            </ul>
          ) : null}
        </>
      )}
    </Modal>
  );
}
