import { useState } from "react";
import { KeyRound } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  useDiscoverMcp,
  type DiscoverMcpReach,
  type DiscoverMcpServer,
} from "@/entities/discover";
import { extensionScopeKey } from "@/entities/extension";
import type { ExtensionScopeOption } from "@/features/extension-management";
import { toErrorCopy } from "@/shared/lib/nativeError";
import { Badge } from "@/shared/ui/Badge";
import { DiscoverAddControl } from "./DiscoverAddControl";
import { DiscoverCard } from "./DiscoverCard";
import { DiscoverFrame } from "./DiscoverFrame";
import { DiscoverLogo } from "./DiscoverLogo";
import {
  inputLabel,
  requiredInputs,
  runsLabel,
  serverDescription,
} from "./discoverLabels";
import { McpDiscoverSheet } from "./McpDiscoverSheet";
import { useDiscoverAdds } from "./useDiscoverAdds";
import { useInstallDiscoveredMcp } from "./useDiscoverMutations";
import type { DiscoverSearch } from "./useDiscoverSearch";

export interface McpDiscoverProps {
  headingId: string;
  search: DiscoverSearch;
  visible: boolean;
  targets: readonly ExtensionScopeOption[];
  blocked: boolean;
}

/** Which of the page's apps can run this server's transport. */
function reachOf(
  server: DiscoverMcpServer,
  targets: readonly ExtensionScopeOption[],
  reach: readonly DiscoverMcpReach[],
) {
  const reachable = targets.filter((target) =>
    reach
      .find(
        (entry) =>
          extensionScopeKey(entry.scope) === extensionScopeKey(target.scope),
      )
      ?.transports.includes(server.transport),
  );
  const unreachable = targets
    .filter((target) => !reachable.includes(target))
    .map((target) => target.name);
  return { reachable, unreachable };
}

export function McpDiscover({
  headingId,
  search,
  visible,
  targets,
  blocked,
}: McpDiscoverProps) {
  const { t } = useTranslation();
  const list = useDiscoverMcp(search.query, visible);
  const adds = useDiscoverAdds();
  const install = useInstallDiscoveredMcp();
  const [opened, setOpened] = useState<DiscoverMcpServer | null>(null);
  const items = list.data?.items ?? [];
  const reach = list.data?.reach ?? [];

  const add = (
    server: DiscoverMcpServer,
    values: Record<string, string>,
    scopes: readonly ExtensionScopeOption[],
    quiet: boolean,
  ) =>
    adds.start(
      server.id,
      server.title,
      () =>
        install.mutateAsync({
          server: server.id,
          values: Object.entries(values)
            .filter(([, value]) => value.trim() !== "")
            .map(([key, value]) => ({ key, value })),
          description: serverDescription(t, server),
          scopes: scopes.map((target) => target.scope),
        }),
      quiet,
    );

  const sourceError = list.data?.sourceError
    ? t(list.data.sourceError.messageKey)
    : null;
  const failure = adds.failure
    ? t("discover.addFailed", {
        name: adds.failure.name,
        reason: t(toErrorCopy(adds.failure.error).messageKey),
      })
    : null;
  const listError = list.isError ? t(toErrorCopy(list.error).messageKey) : null;
  const opening = opened
    ? (items.find((item) => item.id === opened.id) ?? opened)
    : null;

  return (
    <>
      <DiscoverFrame
        headingId={headingId}
        note={
          search.query && list.data
            ? t("discover.results", { count: items.length })
            : t("discover.source.mcp")
        }
        search={search}
        searchLabel={t("discover.search.mcp")}
        placeholder={t("discover.search.mcpPlaceholder")}
        loading={!list.data && !list.isError}
        refreshing={list.isFetching && Boolean(list.data)}
        message={failure ?? listError ?? sourceError}
        empty={
          list.data && items.length === 0
            ? search.query
              ? t("discover.noMatch", { query: search.query })
              : t("discover.empty")
            : null
        }
      >
        {items.map((server) => {
          const needs = requiredInputs(server);
          const { reachable } = reachOf(server, targets, reach);
          const state = adds.stateOf(server.id, server.added !== null);
          return (
            <DiscoverCard
              key={server.id}
              title={server.title}
              detailsLabel={t("discover.details", { name: server.title })}
              logo={<DiscoverLogo url={server.icon} name={server.title} />}
              meta={
                <>
                  {server.publisher ? (
                    <span className="truncate">{server.publisher}</span>
                  ) : null}
                  <Badge className="h-4 px-1.5">{runsLabel(t, server)}</Badge>
                  {needs.length > 0 ? (
                    <KeyRound
                      role="img"
                      className="h-3.5 w-3.5 shrink-0"
                      aria-label={t("discover.needs", {
                        what: needs.map((input) => inputLabel(t, input)),
                      })}
                    />
                  ) : null}
                </>
              }
              description={serverDescription(t, server)}
              footer={<span className="truncate font-mono">{server.name}</span>}
              action={
                <DiscoverAddControl
                  name={server.title}
                  state={state}
                  addedAs={server.added}
                  needsInput={needs.length > 0}
                  disabled={blocked}
                  onAdd={() => {
                    if (needs.length > 0 || reachable.length === 0) {
                      setOpened(server);
                    } else {
                      void add(server, {}, reachable, false);
                    }
                  }}
                />
              }
              onOpen={() => setOpened(server)}
            />
          );
        })}
      </DiscoverFrame>
      {opening ? (
        <McpDiscoverSheet
          key={opening.id}
          server={opening}
          {...reachOf(opening, targets, reach)}
          state={adds.stateOf(opening.id, opening.added !== null)}
          blocked={blocked}
          onClose={() => setOpened(null)}
          onAdd={(values, scopes) => add(opening, values, scopes, true)}
        />
      ) : null}
    </>
  );
}
