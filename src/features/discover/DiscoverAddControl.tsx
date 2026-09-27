import { Check } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/Tooltip";
import type { DiscoverAddState } from "./useDiscoverAdds";

export interface DiscoverAddControlProps {
  name: string;
  state: DiscoverAddState;
  /** The existing item's name, when it is already here. */
  addedAs: string | null;
  /** Something must be filled in first, so Add opens the details. */
  needsInput: boolean;
  disabled: boolean;
  onAdd: () => void;
}

/** Add, Adding… or Added, in the card's corner. */
export function DiscoverAddControl({
  name,
  state,
  addedAs,
  needsInput,
  disabled,
  onAdd,
}: DiscoverAddControlProps) {
  const { t } = useTranslation();

  if (state === "added") {
    const label = t("discover.added");
    const content = (
      <span className="inline-flex h-7 items-center gap-1 px-1 text-caption text-content-muted">
        <Check className="h-3.5 w-3.5 text-success" aria-hidden="true" />
        {label}
      </span>
    );
    if (!addedAs) return content;
    return (
      <Tooltip>
        <TooltipTrigger asChild>
          <span
            tabIndex={0}
            aria-label={t("discover.addedAs", { name: addedAs })}
          >
            {content}
          </span>
        </TooltipTrigger>
        <TooltipContent side="bottom">
          {t("discover.addedAs", { name: addedAs })}
        </TooltipContent>
      </Tooltip>
    );
  }

  return (
    <Button
      size="xs"
      variant="secondary"
      loading={state === "adding"}
      disabled={disabled}
      aria-label={t("discover.addNamed", { name })}
      onClick={onAdd}
    >
      {state === "adding"
        ? t("discover.adding")
        : needsInput
          ? t("discover.addWithDetails")
          : t("discover.add")}
    </Button>
  );
}
