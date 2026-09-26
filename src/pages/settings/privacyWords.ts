/** What the words field accepts between words (ADR-0049). */
const SEPARATORS = /[,，、\r\n]/;
const MIN_WORD_CHARS = 2;

/**
 * Splits the field the way the backend does: commas (ASCII, fullwidth or
 * ideographic) and line breaks separate words, each word is trimmed, and
 * words under two characters or repeated are dropped. The backend normalizes
 * again and enforces the limits; this only decides whether anything changed.
 */
export function parsePrivacyWords(text: string): string[] {
  const words: string[] = [];
  for (const piece of text.split(SEPARATORS)) {
    const word = piece.trim();
    if ([...word].length >= MIN_WORD_CHARS && !words.includes(word)) {
      words.push(word);
    }
  }
  return words;
}

export function formatPrivacyWords(words: readonly string[]): string {
  return words.join(", ");
}

export function sameWords(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((word, index) => word === b[index]);
}
