import {
  Activity,
  AlertTriangle,
  Brain,
  CheckCircle2,
  Circle,
  FileCode2,
  Heart,
  IdCard,
  Power,
  Rocket,
  UserRound,
  Wrench,
  type LucideIcon,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type {
  OpenClawWorkspaceFileId,
  OpenClawWorkspaceFileSummary,
  OpenClawWorkspaceFileStatus,
} from "@/entities/openclaw-workspace";
import { Badge, type BadgeTone } from "@/shared/ui/Badge";
import { Card } from "@/shared/ui/Card";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";

const FILE_ICONS: Record<OpenClawWorkspaceFileId, LucideIcon> = {
  agents: FileCode2,
  soul: Heart,
  user: UserRound,
  identity: IdCard,
  tools: Wrench,
  memory: Brain,
  heartbeat: Activity,
  bootstrap: Rocket,
  boot: Power,
};

const STATUS: Record<
  OpenClawWorkspaceFileStatus,
  { tone: BadgeTone; icon: LucideIcon }
> = {
  ready: { tone: "success", icon: CheckCircle2 },
  missing: { tone: "neutral", icon: Circle },
  unavailable: { tone: "warning", icon: AlertTriangle },
};

interface WorkspaceFilesGridProps {
  files: OpenClawWorkspaceFileSummary[];
  onEdit: (file: OpenClawWorkspaceFileSummary) => void;
}

export function WorkspaceFilesGrid({ files, onEdit }: WorkspaceFilesGridProps) {
  const { t } = useTranslation();
  return (
    <section className="flex min-w-0 flex-col gap-4">
      <SectionHeader
        title={t("openClawWorkspace.files.title")}
        description={t("openClawWorkspace.files.description")}
      />
      <div className="grid min-w-0 gap-3 sm:grid-cols-2 xl:grid-cols-3">
        {files.map((file) => {
          const Icon = FILE_ICONS[file.id];
          const status = STATUS[file.status];
          const disabled = file.status === "unavailable";
          return (
            <Card key={file.id} padding="none" className="overflow-hidden">
              <button
                type="button"
                disabled={disabled}
                onClick={() => onEdit(file)}
                className={cn(
                  "group flex min-h-[132px] w-full items-start gap-4 p-5 text-left",
                  "transition-colors duration-fast ease-standard hover:bg-layer-2",
                  "disabled:cursor-not-allowed disabled:opacity-65",
                  FOCUS_RING,
                )}
              >
                <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-xl border border-hairline bg-layer-1 text-brand shadow-sm">
                  <Icon className="h-5 w-5" aria-hidden="true" />
                </span>
                <span className="min-w-0 flex-1">
                  <span className="flex flex-wrap items-center justify-between gap-2">
                    <span className="text-body font-medium text-content">
                      {file.filename}
                    </span>
                    <Badge tone={status.tone} icon={status.icon}>
                      {t(`openClawWorkspace.files.status.${file.status}`)}
                    </Badge>
                  </span>
                  <span className="mt-2 block text-caption leading-5 text-content-muted">
                    {t(`openClawWorkspace.files.descriptionById.${file.id}`)}
                  </span>
                </span>
              </button>
            </Card>
          );
        })}
      </div>
    </section>
  );
}
