import {
  MAX_PROMPT_CONTENT_BYTES,
  MAX_PROMPT_DESCRIPTION_CHARS,
  MAX_PROMPT_NAME_CHARS,
  type PromptDraft,
} from "@/native";

export interface PromptFormValues {
  name: string;
  description: string;
  content: string;
}

export type PromptFormError = "required" | "tooLong" | "invalid" | "tooLarge";

export interface PromptFormErrors {
  name?: PromptFormError;
  description?: PromptFormError;
  content?: PromptFormError;
}

export const EMPTY_PROMPT_FORM: PromptFormValues = {
  name: "",
  description: "",
  content: "",
};

const hasControl = (value: string): boolean =>
  [...value].some((character) => {
    const code = character.codePointAt(0) ?? 0;
    return code <= 31 || (code >= 127 && code <= 159);
  });

export function validatePromptForm(values: PromptFormValues): PromptFormErrors {
  const errors: PromptFormErrors = {};
  const name = values.name.trim();
  if (!name) errors.name = "required";
  else if ([...name].length > MAX_PROMPT_NAME_CHARS) errors.name = "tooLong";
  else if (hasControl(name)) errors.name = "invalid";

  const description = values.description.trim();
  if ([...description].length > MAX_PROMPT_DESCRIPTION_CHARS)
    errors.description = "tooLong";
  else if (hasControl(description)) errors.description = "invalid";

  if (!values.content.trim()) errors.content = "required";
  else if (values.content.includes("\0")) errors.content = "invalid";
  else if (
    new TextEncoder().encode(values.content).byteLength >
    MAX_PROMPT_CONTENT_BYTES
  )
    errors.content = "tooLarge";
  return errors;
}

export function promptDraftFrom(values: PromptFormValues): PromptDraft {
  return {
    name: values.name,
    description: values.description || null,
    content: values.content,
  };
}
