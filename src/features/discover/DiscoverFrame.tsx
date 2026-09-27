import type { ReactNode } from "react";
import { LoaderCircle, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { cn } from "@/shared/ui/cn";
import { Input } from "@/shared/ui/Input";
import { DiscoverCardSkeleton } from "./DiscoverCard";
import type { DiscoverSearch } from "./useDiscoverSearch";

export interface DiscoverFrameProps {
  headingId: string;
  /** Right of the heading: the result count, or where the cards come from. */
  note: string;
  search: DiscoverSearch;
  searchLabel: string;
  placeholder: string;
  /** No answer yet: placeholder cards stand in. */
  loading: boolean;
  /** A newer answer is on its way; the current cards dim. */
  refreshing: boolean;
  /** One quiet line, such as a source that could not be reached. */
  message: string | null;
  /** Shown instead of cards when an answer holds none. */
  empty: string | null;
  children: ReactNode;
}

const SKELETONS = [0, 1, 2, 3, 4, 5];

/** Heading, search box and grid shared by the Skill and MCP sections. */
export function DiscoverFrame({
  headingId,
  note,
  search,
  searchLabel,
  placeholder,
  loading,
  refreshing,
  message,
  empty,
  children,
}: DiscoverFrameProps) {
  const { t } = useTranslation();

  return (
    <>
      <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
        <h2 id={headingId} className="text-heading text-content">
          {t("discover.title")}
        </h2>
        <span aria-live="polite" className="text-caption text-content-muted">
          {note}
        </span>
      </div>
      <form
        role="search"
        className="relative min-w-0"
        onSubmit={(event) => {
          event.preventDefault();
          search.searchNow();
        }}
      >
        <Search
          className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-content-muted"
          aria-hidden="true"
        />
        <Input
          type="search"
          value={search.input}
          maxLength={200}
          autoComplete="off"
          spellCheck={false}
          className="pl-9 pr-9 [&::-webkit-search-cancel-button]:appearance-none"
          aria-label={searchLabel}
          placeholder={placeholder}
          onChange={(event) => search.setInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Escape" && search.input !== "") {
              event.preventDefault();
              event.stopPropagation();
              search.clear();
            }
          }}
        />
        {refreshing ? (
          <LoaderCircle
            className="absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-content-muted motion-safe:animate-spin"
            aria-hidden="true"
          />
        ) : null}
      </form>
      {message ? (
        <p role="status" className="text-caption text-warning">
          {message}
        </p>
      ) : null}
      {loading ? (
        <div
          aria-busy="true"
          aria-label={t("discover.loading")}
          className="grid gap-2.5 [grid-template-columns:repeat(auto-fill,minmax(15rem,1fr))]"
        >
          {SKELETONS.map((index) => (
            <DiscoverCardSkeleton key={index} />
          ))}
        </div>
      ) : empty ? (
        <p className="py-6 text-center text-caption text-content-muted">
          {empty}
        </p>
      ) : (
        <div
          role="list"
          aria-labelledby={headingId}
          aria-busy={refreshing || undefined}
          className={cn(
            "grid gap-2.5 [grid-template-columns:repeat(auto-fill,minmax(15rem,1fr))]",
            "transition-opacity duration-fast ease-standard",
            refreshing && "opacity-60",
          )}
        >
          {children}
        </div>
      )}
    </>
  );
}
