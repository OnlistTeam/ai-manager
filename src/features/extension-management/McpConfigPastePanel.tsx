import { ClipboardPaste } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/shared/ui/Button";
import { Field } from "@/shared/ui/Field";
import { Textarea } from "@/shared/ui/Textarea";
import { parseMcpConfig, type McpPastedConnection } from "./mcpConfigPaste";

const PASTE_ID = "mcp-install-paste";

interface McpConfigPastePanelProps {
  disabled: boolean;
  onApply: (pasted: McpPastedConnection) => void;
}

/**
 * The secondary way in: paste the JSON a README shows and have it fill the
 * form below. Closed by default, so the first view stays free of JSON.
 */
export function McpConfigPastePanel({
  disabled,
  onApply,
}: McpConfigPastePanelProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [text, setText] = useState("");
  const [invalid, setInvalid] = useState(false);
  const [filled, setFilled] = useState<McpPastedConnection | null>(null);
  const textRef = useRef<HTMLTextAreaElement>(null);
  const openRef = useRef<HTMLButtonElement>(null);
  const returnFocus = useRef(false);

  useEffect(() => {
    if (open) {
      textRef.current?.focus();
    } else if (returnFocus.current) {
      returnFocus.current = false;
      openRef.current?.focus();
    }
  }, [open]);

  const close = () => {
    setOpen(false);
    setText("");
    setInvalid(false);
  };
  const apply = () => {
    const pasted = parseMcpConfig(text);
    if (!pasted) {
      setInvalid(true);
      textRef.current?.focus();
      return;
    }
    close();
    setFilled(pasted);
    onApply(pasted);
  };

  if (!open) {
    let status: string | null = null;
    if (filled && filled.total > 1) {
      status = t("extensions.mcp.install.paste.filledFirst", {
        name: filled.name,
      });
    } else if (filled) {
      status = t("extensions.mcp.install.paste.filled");
    }
    return (
      <div className="flex items-center justify-between gap-3">
        <p role="status" className="text-caption text-content-muted">
          {status}
        </p>
        <Button
          ref={openRef}
          variant="ghost"
          size="xs"
          className="shrink-0"
          disabled={disabled}
          onClick={() => {
            setFilled(null);
            setOpen(true);
          }}
        >
          <ClipboardPaste className="h-3.5 w-3.5" aria-hidden="true" />
          {t("extensions.mcp.install.paste.open")}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-hairline bg-layer-1 p-3">
      <Field
        id={PASTE_ID}
        label={t("extensions.mcp.install.paste.label")}
        hint={t("extensions.mcp.install.paste.hint")}
        error={invalid ? t("extensions.mcp.install.paste.invalid") : undefined}
      >
        <Textarea
          ref={textRef}
          id={PASTE_ID}
          rows={6}
          value={text}
          spellCheck={false}
          invalid={invalid}
          disabled={disabled}
          className="font-mono text-caption"
          onChange={(event) => {
            setText(event.target.value);
            setInvalid(false);
          }}
        />
      </Field>
      <div className="flex justify-end gap-2">
        <Button
          variant="ghost"
          size="sm"
          disabled={disabled}
          onClick={() => {
            returnFocus.current = true;
            close();
          }}
        >
          {t("ds.action.cancel")}
        </Button>
        <Button
          variant="secondary"
          size="sm"
          disabled={disabled || !text.trim()}
          onClick={apply}
        >
          {t("extensions.mcp.install.paste.apply")}
        </Button>
      </div>
    </div>
  );
}
