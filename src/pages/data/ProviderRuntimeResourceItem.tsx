import {
  BookOpenText,
  BrainCircuit,
  FileCode2,
  FolderClock,
  FolderOpen,
  PencilLine,
  SquareArrowOutUpRight,
  UserRound,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ProviderRuntimeResource } from "@/entities/provider";
import { Button } from "@/shared/ui/Button";
import { formatStorageBytes } from "./ProviderRuntimeStorageSummary";

/**
 * `managedDescription` is what the item says once the product itself owns the
 * file; `null` means no management surface exists for that kind, so the item
 * can only ever locate or open it.
 */
const resourcePresentation = {
  configuration: {
    icon: FileCode2,
    title: "services.runtime.resource.configuration.title",
    description: "services.runtime.resource.configuration.description",
    managedDescription: null,
  },
  instructions: {
    icon: BookOpenText,
    title: "services.runtime.resource.instructions.title",
    description: "services.runtime.resource.instructions.description",
    managedDescription:
      "services.runtime.resource.instructions.managedDescription",
  },
  memory: {
    icon: BrainCircuit,
    title: "services.runtime.resource.memory.title",
    description: "services.runtime.resource.memory.description",
    managedDescription: null,
  },
  userProfile: {
    icon: UserRound,
    title: "services.runtime.resource.userProfile.title",
    description: "services.runtime.resource.userProfile.description",
    managedDescription: null,
  },
  sessionData: {
    icon: FolderClock,
    title: "services.runtime.resource.sessionData.title",
    description: "services.runtime.resource.sessionData.description",
    managedDescription: null,
  },
} as const;

interface ProviderRuntimeResourceItemProps {
  resource: ProviderRuntimeResource;
  busy: boolean;
  onOpen: () => void;
  /**
   * Given only when the product itself manages this file, in which case the
   * item hands the user over there instead of opening a second write path to
   * the same bytes (ADR-0037).
   */
  onManage?: (() => void) | null;
}

export function ProviderRuntimeResourceItem({
  resource,
  busy,
  onOpen,
  onManage = null,
}: ProviderRuntimeResourceItemProps) {
  const { t, i18n } = useTranslation();
  const presentation = resourcePresentation[resource.kind];
  const Icon = presentation.icon;
  const { managedDescription } = presentation;
  // When this kind has a management surface and the caller has handed over
  // an entry point, this item's only job is to send the user there.
  const manage = managedDescription === null ? null : onManage;
  const ActionIcon =
    manage !== null
      ? SquareArrowOutUpRight
      : resource.action === "edit"
        ? PencilLine
        : FolderOpen;
  const actionLabel =
    manage !== null
      ? "services.runtime.action.manage"
      : resource.action === "edit" && !resource.exists
        ? "services.runtime.action.create"
        : `services.runtime.action.${resource.action}`;
  const description =
    manage !== null && managedDescription !== null
      ? managedDescription
      : presentation.description;
  const locale = i18n.resolvedLanguage || i18n.language || "en";
  const size =
    resource.sizeBytes === null
      ? null
      : formatStorageBytes(
          resource.sizeBytes,
          locale,
          resource.measurementLimited,
        );

  return (
    <li className="flex min-w-0 flex-col gap-3 py-4 sm:flex-row sm:items-center">
      <div className="flex min-w-0 flex-1 items-start gap-3">
        <Icon
          className="mt-0.5 h-4 w-4 shrink-0 text-brand"
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <p className="text-body font-medium text-content">
              {t(presentation.title)}
            </p>
            <span className="text-caption text-content-muted">
              {t(`services.runtime.scope.${resource.scope}` as const)}
            </span>
            {size ? (
              <span
                aria-label={t("services.runtime.storage.resourceSize", {
                  size,
                })}
                className="text-caption text-content-muted"
              >
                {size}
              </span>
            ) : resource.exists && resource.measurementLimited ? (
              <span className="text-caption text-warning">
                {t("services.runtime.storage.sizeUnavailable")}
              </span>
            ) : null}
            {!resource.exists ? (
              <span className="text-caption text-warning">
                {t("services.runtime.notCreated")}
              </span>
            ) : null}
          </div>
          <p className="mt-1 text-caption leading-5 text-content-muted">
            {t(description)}
          </p>
          <code className="mt-1.5 block break-all text-mono-sm text-content-muted">
            {resource.path}
          </code>
        </div>
      </div>
      <Button
        variant="secondary"
        size="sm"
        className="shrink-0 self-start sm:self-center"
        loading={busy}
        onClick={manage ?? onOpen}
      >
        <ActionIcon className="h-4 w-4" aria-hidden="true" />
        {t(actionLabel as "services.runtime.action.edit")}
      </Button>
    </li>
  );
}
