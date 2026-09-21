import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { exit } from "@tauri-apps/plugin-process";
import { AlertTriangle, Database, FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/button";

interface DatabaseUpgradeProps {
  payload: {
    path?: string;
    error?: string;
    kind?: string;
    db_version?: number;
    supported_version?: number;
  };
}

/**
 * Fail-closed recovery screen for when the database version is too new.
 *
 * The product's update endpoint, public key, and N→N+1 upgrade path aren't
 * ready yet, so this screen doesn't check for, download, or install updates,
 * and it doesn't send the user to the release page inherited from upstream.
 * The original database is left untouched; the user can still open the
 * product's data directory to back it up, or quit and use an
 * already-approved compatible version.
 */
export function DatabaseUpgrade({ payload }: DatabaseUpgradeProps) {
  const { t } = useTranslation();
  const dbVersion = payload.db_version;
  const supportedVersion = payload.supported_version;

  return (
    <div className="flex min-h-screen items-center justify-center bg-background p-6 text-foreground">
      <div className="w-full max-w-lg space-y-5 rounded-2xl border border-border/60 bg-card/80 p-7 shadow-xl">
        <div className="flex items-start gap-4">
          <div className="flex h-12 w-12 shrink-0 items-center justify-center rounded-xl bg-amber-100 text-amber-600 dark:bg-amber-950/50 dark:text-amber-400">
            <Database className="h-6 w-6" aria-hidden="true" />
          </div>
          <div className="space-y-1">
            <h1 className="text-lg font-semibold">{t("dbUpgrade.title")}</h1>
            <p className="text-sm text-muted-foreground">
              {t("dbUpgrade.description")}
            </p>
            {dbVersion != null && supportedVersion != null && (
              <p className="pt-0.5 text-xs tabular-nums text-muted-foreground">
                {t("dbUpgrade.versionInfo", {
                  db: dbVersion,
                  supported: supportedVersion,
                })}
              </p>
            )}
          </div>
        </div>

        <div className="space-y-1 rounded-lg border border-border/50 bg-muted/40 p-3 text-xs text-muted-foreground">
          {payload.error && (
            <p className="break-words font-mono">{payload.error}</p>
          )}
          {payload.path && (
            <p className="break-all">
              {t("dbUpgrade.dbPath")}：{payload.path}
            </p>
          )}
        </div>

        <div className="space-y-2 rounded-lg border border-amber-300/60 bg-amber-50 p-3 text-sm text-amber-800 dark:border-amber-500/40 dark:bg-amber-950/40 dark:text-amber-200">
          <p className="flex items-center gap-2 font-medium">
            <AlertTriangle className="h-4 w-4 shrink-0" aria-hidden="true" />
            {t("dbUpgrade.channelUnavailableTitle")}
          </p>
          <p className="leading-relaxed">
            {t("dbUpgrade.channelUnavailableDescription")}
          </p>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <Button
            variant="outline"
            className="gap-2"
            onClick={() => void invoke("open_app_config_folder")}
          >
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            {t("dbUpgrade.openConfigDir")}
          </Button>

          <Button
            variant="ghost"
            className="ml-auto text-muted-foreground"
            onClick={() => void exit(0)}
          >
            {t("dbUpgrade.quit")}
          </Button>
        </div>
      </div>
    </div>
  );
}

export default DatabaseUpgrade;
