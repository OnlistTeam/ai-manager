import { Check, Gauge, LoaderCircle, Plus, Search } from "lucide-react";
import { useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type {
  ProviderConnectionPreset,
  ProviderConnectionProfile,
  ProviderEndpointTestResult,
  ProviderPresetKind,
  ToolId,
  ToolLoginStatus,
} from "@/entities/provider";
import { Button } from "@/shared/ui/Button";
import { Input } from "@/shared/ui/Input";
import { ServiceLogo } from "@/shared/ui/ServiceArtwork";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { hostOf } from "./effectiveConnectionCopy";
import { toolLoginSummary } from "./toolLoginCopy";
import { useProviderPresetSpeedTest } from "./useProviderPresetSpeedTest";

/** Group order on the page; `login` (the tool's own account) comes first. */
const PRESET_GROUPS: readonly ProviderPresetKind[] = [
  "vendor",
  "relay",
  "local",
];

export interface ProviderAddPageProps {
  tool: ToolId;
  toolName: string;
  profile: ProviderConnectionProfile;
  /** What the tool says about its own sign-in (ADR-0060). */
  loginStatus?: ToolLoginStatus;
  disabled?: boolean;
  onPickPreset: (preset: ProviderConnectionPreset) => void;
  onPickCustom: () => void;
  onPickToolLogin: () => void;
}

function matches(query: string, ...values: string[]): boolean {
  return values.some((value) => value.toLowerCase().includes(query));
}

/**
 * The second page behind "Add endpoint" (ADR-0057): every service this tool
 * can be pointed at, as cards grouped by what kind of service it is.
 *
 * The cards differ from tool to tool on purpose. A preset is written straight
 * into the tool's own config, so a tool only lists services that speak its
 * protocol; one it cannot use is left out rather than shown disabled.
 */
export function ProviderAddPage({
  tool,
  toolName,
  profile,
  loginStatus,
  disabled = false,
  onPickPreset,
  onPickCustom,
  onPickToolLogin,
}: ProviderAddPageProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const speedTest = useProviderPresetSpeedTest(tool);
  const needle = query.trim().toLowerCase();
  const loginName = profile.toolLogin
    ? t(`services.add.login.${profile.toolLogin}`)
    : null;
  // The card says whether the tool is signed in, not whether its entry is
  // saved: upstream seeds that entry, so a saved mark read as "signed in" to
  // someone who never was (ADR-0060). Signed out, it offers to sign in here
  // (ADR-0061).
  const loginDetail =
    (loginStatus?.state === "signedOut"
      ? null
      : toolLoginSummary(loginStatus, t)) ??
    t("services.add.loginDetail", { tool: toolName });

  const measurements = useMemo(
    () =>
      new Map(speedTest.results.map((result) => [result.candidateId, result])),
    [speedTest.results],
  );
  const groups = useMemo(
    () =>
      PRESET_GROUPS.map((kind) => ({
        kind,
        presets: profile.presets.filter(
          (preset) =>
            preset.kind === kind &&
            (!needle ||
              matches(needle, preset.serviceName, hostOf(preset.baseUrl))),
        ),
      })).filter((group) => group.presets.length > 0),
    [needle, profile.presets],
  );
  const showLogin =
    loginName !== null && (!needle || matches(needle, loginName));
  const nothingFound = needle !== "" && !showLogin && groups.length === 0;

  return (
    <div className="flex min-w-0 flex-col gap-5">
      <div className="flex flex-wrap items-center gap-2">
        <label className="relative min-w-[14rem] flex-1">
          <span className="sr-only">{t("services.add.search")}</span>
          <Search
            className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-content-muted"
            aria-hidden="true"
          />
          <Input
            type="search"
            value={query}
            className="pl-9"
            placeholder={t("services.add.search")}
            onChange={(event) => setQuery(event.target.value)}
          />
        </label>
        <Button
          variant="secondary"
          disabled={disabled || speedTest.testing}
          onClick={speedTest.run}
        >
          {speedTest.testing ? (
            <LoaderCircle className="h-4 w-4 animate-spin" aria-hidden="true" />
          ) : (
            <Gauge className="h-4 w-4 text-brand" aria-hidden="true" />
          )}
          {t(
            speedTest.testing
              ? "services.connect.speedTesting"
              : speedTest.measured
                ? "services.connect.speedTestAgain"
                : "services.connect.speedTest",
          )}
        </Button>
      </div>
      {speedTest.error ? (
        <p className="-mt-3 text-caption text-danger" role="alert">
          {t("services.connect.speedTestFailed")}
        </p>
      ) : null}

      {needle === "" ? (
        <CardGrid>
          <AddCard
            dashed
            mark={
              <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-[5px] bg-brand/10 text-brand">
                <Plus className="h-3.5 w-3.5" aria-hidden="true" />
              </span>
            }
            name={t("services.add.custom")}
            detail={t("services.add.customDetail")}
            disabled={disabled}
            onClick={onPickCustom}
          />
        </CardGrid>
      ) : null}

      {showLogin && profile.toolLogin ? (
        <CardGroup
          title={t("services.add.group.login")}
          hint={t("services.add.groupHint.login", { tool: toolName })}
        >
          <AddCard
            mark={
              <ServiceLogo name={loginName ?? ""} account={profile.toolLogin} />
            }
            name={loginName ?? ""}
            detail={loginDetail}
            trailing={
              loginStatus?.state === "signedIn" ? (
                // The detail line already says it in words.
                <Check
                  className="h-4 w-4 shrink-0 text-brand"
                  aria-hidden="true"
                />
              ) : null
            }
            disabled={disabled}
            onClick={onPickToolLogin}
          />
        </CardGroup>
      ) : null}

      {groups.map((group) => (
        <CardGroup
          key={group.kind}
          title={t(`services.add.group.${group.kind}`)}
          hint={t(`services.add.groupHint.${group.kind}`)}
        >
          {group.presets.map((preset) => (
            <AddCard
              key={preset.id}
              mark={
                <ServiceLogo
                  name={preset.serviceName}
                  urls={[preset.websiteUrl, preset.baseUrl]}
                />
              }
              name={preset.serviceName}
              detail={hostOf(preset.baseUrl)}
              trailing={
                <Latency measurement={measurements.get(preset.id) ?? null} />
              }
              disabled={disabled}
              onClick={() => onPickPreset(preset)}
            />
          ))}
        </CardGroup>
      ))}

      {nothingFound ? (
        <p className="text-body text-content-muted">
          {t("services.add.noMatch", { query: query.trim() })}{" "}
          <button
            type="button"
            disabled={disabled}
            onClick={onPickCustom}
            className={cn(
              "rounded-sm font-medium text-brand hover:text-brand-hover disabled:opacity-55",
              FOCUS_RING,
            )}
          >
            {t("services.add.addAsCustom")}
          </button>
        </p>
      ) : null}
    </div>
  );
}

function CardGrid({ children }: { children: ReactNode }) {
  return (
    <div className="grid grid-cols-[repeat(auto-fill,minmax(190px,1fr))] gap-2">
      {children}
    </div>
  );
}

function CardGroup({
  title,
  hint,
  children,
}: {
  title: string;
  hint: string;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2">
      <h3 className="flex flex-wrap items-baseline gap-x-2 text-caption">
        <span className="font-semibold text-content">{title}</span>
        <span className="text-content-muted">{hint}</span>
      </h3>
      <CardGrid>{children}</CardGrid>
    </section>
  );
}

function AddCard({
  mark,
  name,
  detail,
  trailing = null,
  dashed = false,
  disabled,
  onClick,
}: {
  mark: ReactNode;
  name: string;
  detail: string;
  trailing?: ReactNode;
  dashed?: boolean;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "flex min-w-0 items-center gap-2.5 rounded-lg border bg-layer-1 px-3 py-2 text-left transition-colors hover:border-brand/35 hover:bg-brand/[0.05] disabled:cursor-not-allowed disabled:opacity-55",
        dashed ? "border-dashed border-hairline-strong" : "border-hairline",
        FOCUS_RING,
      )}
    >
      {mark}
      <span className="min-w-0 flex-1 leading-tight">
        <span className="block truncate text-body font-medium text-content">
          {name}
        </span>
        <span className="block truncate text-caption text-content-muted">
          {detail}
        </span>
      </span>
      {trailing}
    </button>
  );
}

function Latency({
  measurement,
}: {
  measurement: ProviderEndpointTestResult | null;
}) {
  const { t } = useTranslation();
  if (!measurement) return null;
  const reached =
    measurement.failure === null && measurement.latencyMs !== null;
  return (
    <span
      className={cn(
        "shrink-0 rounded-full px-1.5 py-0.5 text-caption tabular-nums",
        reached ? "bg-brand/10 text-brand" : "text-content-muted",
      )}
    >
      {reached
        ? t("services.connect.responseTime", {
            latency: measurement.latencyMs,
          })
        : t("services.connect.unreachable")}
    </span>
  );
}
