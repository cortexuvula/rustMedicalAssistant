/** Strip trailing `.,;:!?` from a word for length-based calculations. */
function stripTrailingPunct(word: string): string {
  return word.replace(/[.,;:!?]+$/u, '');
}

/**
 * Returns the Optimal Recognition Point index — the zero-based character
 * position that should be visually highlighted to align the word's "centre
 * of recognition" with a fixed gaze point.
 *
 * Rule (ported from legacy Python):
 *   1-3 chars → 0
 *   4-5 chars → 1
 *   6-9 chars → 2
 *   10+ chars → 3
 */
export function orpIndex(word: string): number {
  const len = stripTrailingPunct(word).length;
  if (len <= 3) return 0;
  if (len <= 5) return 1;
  if (len <= 9) return 2;
  return 3;
}

export type TokenKind = 'word' | 'clause' | 'sentence' | 'header';

export interface Token {
  word: string;
  kind: TokenKind;
}

/** Milliseconds per word at the given WPM. Legacy formula: 60_000 / wpm. */
export function baseDelayMs(wpm: number): number {
  return Math.round(60_000 / wpm);
}

/**
 * Delay to render the given token. Variable timing matches the legacy Python:
 *   word     → 1.0 × base
 *   clause   → 1.5 × base  (trailing `,;:`)
 *   sentence → 2.5 × base  (trailing `.!?`)
 *   header   → 3.0 × base  (a known SOAP section header)
 */
export function delayMs(token: Token, base: number): number {
  const multiplier =
    token.kind === 'header' ? 3.0 :
    token.kind === 'sentence' ? 2.5 :
    token.kind === 'clause' ? 1.5 :
    1.0;
  return Math.round(base * multiplier);
}
