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
