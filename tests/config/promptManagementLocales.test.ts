import { describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import ja from "@/i18n/locales/ja.json";
import zhTw from "@/i18n/locales/zh-TW.json";
import zh from "@/i18n/locales/zh.json";

const locales = { en, ja, "zh-TW": zhTw, zh } as const;

describe("Prompt Basic Management locale contract", () => {
  for (const [locale, messages] of Object.entries(locales)) {
    it(`${locale} covers create, edit, import, remove, validation, and recovery`, () => {
      const prompt = messages.extensions.prompt;
      const values = [
        messages.extensions.card.edit,
        messages.extensions.card.editNamed,
        prompt.editor.add,
        prompt.editor.createTitle,
        prompt.editor.editTitle,
        prompt.editor.activeBackup,
        prompt.editor.readErrorTitle,
        prompt.editor.saveErrorTitle,
        prompt.editor.validation.name.required,
        prompt.editor.validation.name.tooLong,
        prompt.editor.validation.name.invalid,
        prompt.editor.validation.description.tooLong,
        prompt.editor.validation.description.invalid,
        prompt.editor.validation.content.required,
        prompt.editor.validation.content.invalid,
        prompt.editor.validation.content.tooLarge,
        prompt.import.open,
        prompt.import.point.reads,
        prompt.import.point.unchanged,
        prompt.import.point.duplicate,
        prompt.import.errorTitle,
        prompt.remove.point.current,
        prompt.remove.point.file,
        prompt.remove.point.undo,
        prompt.remove.active,
        prompt.remove.errorTitle,
        messages.error.prompt.saveRestoreFailed,
        messages.error.prompt.removeRestoreFailed,
        messages.error.remediation.checkPromptSettings,
        messages.error.remediation.chooseAnotherPrompt,
      ];

      expect(
        values.every((value) => value.trim().length > 0),
        `${locale} has an empty Prompt management message`,
      ).toBe(true);
    });
  }
});
