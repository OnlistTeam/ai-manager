/**
 * Normalizes common Chinese/fullwidth/curly quotes to ASCII quotes, to avoid TOML parse failures.
 * - Double quotes: " " „ ‟ ＂ → "
 * - Single quotes: ' ' ＇ → '
 * To be conservative, this does not replace title marks/corner brackets (《》, 「」, etc.), to avoid mangling content semantics.
 */
export const normalizeQuotes = (text: string): string => {
  if (!text) return text;
  return (
    text
      // Double-quote family -> "
      .replace(/[“”„‟＂]/g, '"')
      // Single-quote family -> '
      .replace(/[‘’＇]/g, "'")
  );
};

/**
 * Normalization dedicated to TOML text; currently identical to normalizeQuotes, may be extended later (e.g. whitespace, line endings, etc.).
 */
export const normalizeTomlText = (text: string): string =>
  normalizeQuotes(text);
