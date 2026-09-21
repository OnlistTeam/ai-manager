import { useEffect, useState } from "react";
import { AlertTriangle, LoaderCircle, Network } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useToolVersionCatalog, type Tool } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { Modal } from "@/shared/ui/Modal";
import { ToolActionError } from "./ToolActionError";
import { VersionSelect } from "./VersionSelect";
import { groupVersionOptions } from "./versionOptions";

export interface VersionManagementModalProps {
  tool: Tool | null;
  busy?: boolean;
  error?: Error | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: (version: string) => void;
}

export function VersionManagementModal({
  tool,
  busy = false,
  error = null,
  onOpenChange,
  onConfirm,
}: VersionManagementModalProps) {
  const { t } = useTranslation();
  const catalog = useToolVersionCatalog(tool?.id ?? null);
  const [selected, setSelected] = useState("");
  const optionGroups = groupVersionOptions(
    catalog.data?.versions ?? [],
    catalog.data?.latestVersion,
    catalog.data?.distTags,
  );
  const initialVersion =
    optionGroups.defaultVersion ??
    optionGroups.taggedPublished[0]?.version ??
    optionGroups.otherPublished[0] ??
    "";

  useEffect(() => {
    setSelected(initialVersion);
  }, [initialVersion, tool?.id]);

  const data = catalog.data;
  const restricted = data !== undefined && !data.canChangeVersion;
  const unchanged = selected !== "" && selected === tool?.version;
  const selectedIsNonRecommended =
    selected !== "" && selected !== data?.latestVersion;
  const canSubmit =
    data?.canChangeVersion === true && selected !== "" && !unchanged;

  return (
    <Modal
      open={tool !== null}
      onOpenChange={onOpenChange}
      dismissible={!busy}
      size="md"
      title={t("tools.version.title", { name: tool?.name ?? "" })}
      description={t("tools.version.description")}
      footer={
        <>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={() => onOpenChange(false)}
          >
            {t(restricted ? "ds.action.close" : "ds.action.cancel")}
          </Button>
          {!restricted ? (
            <Button
              disabled={!canSubmit}
              loading={busy}
              onClick={() => onConfirm(selected)}
            >
              {t("tools.version.confirm")}
            </Button>
          ) : null}
        </>
      }
    >
      <div className="flex flex-col gap-4">
        {catalog.isFetching && !data ? (
          <div className="flex items-center gap-2 rounded-lg border border-hairline bg-layer-1 p-4 text-content-muted">
            <LoaderCircle className="h-4 w-4 animate-spin" aria-hidden="true" />
            <span>{t("tools.version.loading")}</span>
          </div>
        ) : null}

        {catalog.isError && !data ? (
          <div className="flex flex-col gap-3">
            <ToolActionError error={catalog.error} />
            <Button
              variant="secondary"
              disabled={catalog.isFetching}
              loading={catalog.isFetching}
              onClick={() => void catalog.refetch()}
            >
              {t("ds.action.retry")}
            </Button>
          </div>
        ) : null}

        {data ? (
          <>
            <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 rounded-lg border border-hairline bg-layer-1 p-4 text-body">
              <dt className="text-content-muted">
                {t("tools.version.current")}
              </dt>
              <dd className="break-all font-mono text-mono-sm text-content">
                {tool?.version ?? t("tools.version.notInstalled")}
              </dd>
              <dt className="text-content-muted">
                {t("tools.version.sourceLabel")}
              </dt>
              <dd className="text-content">
                {t(`tools.version.source.${data.source}`)}
              </dd>
            </dl>

            {restricted && data.restriction ? (
              <div className="flex items-start gap-2 rounded-lg bg-warning/10 p-4 text-body text-content">
                <AlertTriangle
                  className="mt-0.5 h-4 w-4 shrink-0 text-warning"
                  aria-hidden="true"
                />
                <div>
                  <p className="font-medium">
                    {t("tools.version.restrictedTitle")}
                  </p>
                  <p className="mt-1 text-caption text-content-muted">
                    {t(`tools.version.restriction.${data.restriction}`)}
                  </p>
                </div>
              </div>
            ) : null}

            {data.canChangeVersion ? (
              <div>
                <label
                  htmlFor="tool-version-target"
                  className="text-body font-medium text-content"
                >
                  {t("tools.version.target")}
                </label>
                <div className="mt-2">
                  <VersionSelect
                    id="tool-version-target"
                    value={selected}
                    disabled={busy}
                    groups={optionGroups}
                    onValueChange={setSelected}
                  />
                </div>
                <p className="mt-2 text-caption text-content-muted">
                  {selectedIsNonRecommended
                    ? t("tools.version.nonDefaultWarning")
                    : unchanged && selected === data.latestVersion
                      ? t("tools.version.currentDefault")
                      : unchanged
                        ? t("tools.version.unchanged")
                        : t("tools.version.ownerPreserved")}
                </p>
                {optionGroups.otherPublished.length > 0 &&
                data.latestVersion ? (
                  <p className="mt-2 rounded-lg border border-warning/20 bg-warning/8 px-3 py-2 text-caption leading-5 text-content-muted">
                    {t("tools.version.defaultChannelNote", {
                      version: data.latestVersion,
                    })}
                  </p>
                ) : null}
              </div>
            ) : null}

            {data.mirrorUsed ? (
              <div className="flex items-start gap-2 rounded-lg bg-brand/10 p-3 text-caption text-content-muted">
                <Network
                  className="mt-0.5 h-4 w-4 shrink-0 text-brand"
                  aria-hidden="true"
                />
                <span>{t("tools.version.mirrorUsed")}</span>
              </div>
            ) : null}
          </>
        ) : null}

        {error ? <ToolActionError error={error} /> : null}
      </div>
    </Modal>
  );
}
