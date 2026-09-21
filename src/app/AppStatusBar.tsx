import { ShieldCheck } from "lucide-react";
import { useTranslation } from "react-i18next";
import { TaskCenter } from "@/features/task-center";
import { GlobalUpdateAction } from "@/features/updater";

export interface AppStatusBarProps {
  mac: boolean;
}

/** Persistent, integrated chrome shared by every route. */
export function AppStatusBar({ mac }: AppStatusBarProps) {
  const { t } = useTranslation();

  return (
    <header
      data-app-statusbar=""
      data-integrated="true"
      className={
        mac
          ? "app-statusbar pointer-events-none absolute inset-x-0 top-0 z-30 flex h-[92px] items-start justify-end bg-transparent px-6 pt-7 lg:px-8"
          : "app-statusbar pointer-events-none absolute inset-x-0 top-0 z-30 flex h-[76px] items-start justify-end bg-transparent px-6 pt-4 lg:px-8"
      }
    >
      <div
        data-tauri-no-drag=""
        className="app-statusbar__controls pointer-events-auto flex items-center gap-3"
      >
        <span className="app-statusbar__privacy inline-flex items-center gap-2 whitespace-nowrap text-caption text-content-muted">
          <ShieldCheck
            className="h-3.5 w-3.5 text-success"
            aria-hidden="true"
          />
          {t("nav.localFirst")}
        </span>
        <GlobalUpdateAction />
        <TaskCenter />
      </div>
    </header>
  );
}
