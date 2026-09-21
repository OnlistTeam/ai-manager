import { RefreshCw, Search } from "lucide-react";
import type { FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { Input } from "@/shared/ui/Input";

interface SessionsToolbarProps {
  input: string;
  refreshing: boolean;
  onInput: (value: string) => void;
  onRefresh: () => void;
}

export function SessionsToolbar(props: SessionsToolbarProps) {
  const { t } = useTranslation();
  /** Typing already searches; Enter has no extra meaning — this just stops the webview from actually submitting the form. */
  function submit(event: FormEvent): void {
    event.preventDefault();
  }

  return (
    <form
      role="search"
      onSubmit={submit}
      className="grid gap-2 rounded-xl border border-hairline bg-layer-1 p-3 shadow-sm sm:grid-cols-[1fr_auto]"
    >
      <div className="relative min-w-0">
        <Search
          className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-content-muted"
          aria-hidden="true"
        />
        <Input
          value={props.input}
          maxLength={200}
          className="pl-9"
          aria-label={t("sessions.search.label")}
          placeholder={t("sessions.search.placeholder")}
          onChange={(event) => props.onInput(event.target.value)}
        />
      </div>
      <Button
        type="button"
        variant="secondary"
        loading={props.refreshing}
        onClick={props.onRefresh}
      >
        <RefreshCw className="h-4 w-4" aria-hidden="true" />
        {t("sessions.refresh")}
      </Button>
    </form>
  );
}
