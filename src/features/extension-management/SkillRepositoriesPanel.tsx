import { AlertCircle, Plus, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { useSkillRepositories } from "@/entities/skill-repository";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { Input } from "@/shared/ui/Input";
import {
  EMPTY_SKILL_REPOSITORY_VALUES,
  skillRepositoryDraftFrom,
  validateSkillRepository,
  type SkillRepositoryFormErrors,
  type SkillRepositoryValues,
} from "./skillRepositoryForm";
import { SkillRepositoryRow } from "./SkillRepositoryRow";
import {
  useRemoveSkillRepository,
  useSaveSkillRepository,
} from "./useSkillRepositories";

export function SkillRepositoriesPanel() {
  const { t } = useTranslation();
  const locationRef = useRef<HTMLInputElement>(null);
  const [values, setValues] = useState<SkillRepositoryValues>(
    EMPTY_SKILL_REPOSITORY_VALUES,
  );
  const [errors, setErrors] = useState<SkillRepositoryFormErrors>({});
  const repositories = useSkillRepositories();
  const save = useSaveSkillRepository();
  const remove = useRemoveSkillRepository();
  const dataAvailable = repositories.data !== undefined;
  const busy =
    repositories.isFetching ||
    repositories.isError ||
    save.isPending ||
    remove.isPending;
  const mutationError = remove.error ?? save.error;
  const errorCopy = mutationError ? toErrorCopy(mutationError) : null;

  useEffect(() => {
    locationRef.current?.focus();
  }, []);

  const change = (field: keyof SkillRepositoryValues, value: string) => {
    setValues((current) => ({ ...current, [field]: value }));
    if (field === "location") setErrors({});
    save.reset();
    remove.reset();
  };
  const submit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const nextErrors = validateSkillRepository(values);
    setErrors(nextErrors);
    if (nextErrors.location) {
      locationRef.current?.focus();
      return;
    }
    remove.reset();
    save.mutate(skillRepositoryDraftFrom(values), {
      onSuccess: () => {
        setValues(EMPTY_SKILL_REPOSITORY_VALUES);
        setErrors({});
        locationRef.current?.focus();
      },
    });
  };
  const saveRepository = (draft: Parameters<typeof save.mutate>[0]): void => {
    remove.reset();
    save.mutate(draft);
  };
  const removeRepository = (
    repository: Parameters<typeof remove.mutate>[0],
  ): void => {
    save.reset();
    remove.mutate(repository);
  };

  return (
    <div className="flex flex-col gap-4">
      <form
        className="grid gap-3 rounded-xl border border-hairline bg-layer-1 p-4 sm:grid-cols-[minmax(0,1fr)_minmax(9rem,0.45fr)_auto] sm:items-end"
        onSubmit={submit}
      >
        <label className="min-w-0 text-caption font-medium text-content">
          {t("extensions.skill.repositories.location")}
          <Input
            ref={locationRef}
            className="mt-1.5"
            value={values.location}
            disabled={save.isPending}
            aria-invalid={errors.location ? true : undefined}
            aria-describedby={
              errors.location ? "skill-repository-location-error" : undefined
            }
            placeholder={t("extensions.skill.repositories.locationPlaceholder")}
            onChange={(event) => change("location", event.target.value)}
          />
          {errors.location ? (
            <span
              id="skill-repository-location-error"
              role="alert"
              className="mt-1 block text-caption font-normal text-danger"
            >
              {t(errors.location)}
            </span>
          ) : null}
        </label>
        <label className="min-w-0 text-caption font-medium text-content">
          {t("extensions.skill.repositories.branch")}
          <Input
            className="mt-1.5"
            value={values.branch}
            disabled={save.isPending}
            placeholder={t("extensions.skill.repositories.branchPlaceholder")}
            onChange={(event) => change("branch", event.target.value)}
          />
        </label>
        <Button
          type="submit"
          loading={save.isPending}
          disabled={busy && !save.isPending}
        >
          <Plus className="h-4 w-4" aria-hidden="true" />
          {t("extensions.skill.repositories.add")}
        </Button>
      </form>

      {repositories.isPending && !repositories.isFetched ? (
        <DetectionStatus
          label={t("extensions.skill.repositories.loading")}
          className="min-h-24"
        />
      ) : null}

      {repositories.isError ? (
        <div
          role="alert"
          aria-label={t("extensions.skill.repositories.errorTitle")}
          className="flex items-center justify-between gap-4 rounded-xl border border-warning/30 bg-warning/10 px-4 py-3"
        >
          <div className="flex min-w-0 items-start gap-2">
            <AlertCircle
              className="mt-0.5 h-4 w-4 shrink-0 text-warning"
              aria-hidden="true"
            />
            <p className="text-caption leading-5 text-content-muted">
              {t("extensions.skill.repositories.errorDescription")}
            </p>
          </div>
          <Button
            size="sm"
            variant="secondary"
            loading={repositories.isFetching}
            onClick={() => void repositories.refetch()}
          >
            <RefreshCw className="h-4 w-4" aria-hidden="true" />
            {t("extensions.skill.repositories.retry")}
          </Button>
        </div>
      ) : null}

      {errorCopy ? (
        <div
          role="alert"
          className="flex items-start gap-2 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3"
        >
          <AlertCircle
            className="mt-0.5 h-4 w-4 shrink-0 text-danger"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="text-caption font-medium text-content">
              {t(errorCopy.messageKey)}
            </p>
            <p className="mt-0.5 text-caption text-content-muted">
              {t("extensions.skill.repositories.mutationErrorHint")}
            </p>
          </div>
        </div>
      ) : null}

      {dataAvailable && repositories.data.length === 0 ? (
        <p className="rounded-xl border border-dashed border-hairline px-4 py-8 text-center text-body text-content-muted">
          {t("extensions.skill.repositories.empty")}
        </p>
      ) : null}

      {dataAvailable && repositories.data.length > 0 ? (
        <ul className="scrollbar-subtle flex max-h-[42vh] flex-col gap-2 overflow-y-auto pr-1">
          {repositories.data.map((repository) => (
            <SkillRepositoryRow
              key={repository.id}
              repository={repository}
              busy={busy}
              onSave={saveRepository}
              onRemove={removeRepository}
            />
          ))}
        </ul>
      ) : null}
    </div>
  );
}
