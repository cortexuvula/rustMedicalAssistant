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

const SOAP_HEADER_NAMES = [
  'subjective',
  'objective',
  'assessment',
  'plan',
  'differential diagnosis',
  'follow up',
  'follow-up',
  'clinical synopsis',
];

/** Matches leading markdown emphasis (*, **, _, __) that LLMs sometimes add. */
const LEADING_EMPHASIS = /^[*_]+/u;
const TRAILING_EMPHASIS = /[*_]+$/u;

function isHeaderWord(word: string): boolean {
  const stripped = word.replace(LEADING_EMPHASIS, '').replace(TRAILING_EMPHASIS, '');
  if (!stripped.endsWith(':')) return false;
  const name = stripped.slice(0, -1).toLowerCase().trim();
  return SOAP_HEADER_NAMES.includes(name);
}

function classify(word: string): TokenKind {
  if (isHeaderWord(word)) return 'header';
  const lastChar = word[word.length - 1];
  if (lastChar === '.' || lastChar === '!' || lastChar === '?') return 'sentence';
  if (lastChar === ',' || lastChar === ';' || lastChar === ':') return 'clause';
  return 'word';
}

/** Split `text` into whitespace-separated tokens, each classified by trailing punctuation. */
export function tokenize(text: string): Token[] {
  return text
    .split(/\s+/u)
    .filter((w) => w.length > 0)
    .map((word) => ({ word, kind: classify(word) }));
}

// Matches: "(ICD-10: X00.0)", "[ICD-9: 250.00]", " ICD-10: J45.909", etc.
const ICD_RE = /\s*[\(\[]?\s*ICD-\d+:?\s*[A-Z]?[\d\.]+\s*[\)\]]?/giu;
const NOT_DISCUSSED_LINE_RE = /^.*?:\s*Not discussed.*$/gimu;
const LEADING_BULLET_RE = /^[-•*]\s+/gmu;

/**
 * Clean SOAP text for speed-reading:
 *   - Strip ICD-9 / ICD-10 code fragments (they slow reading without adding meaning)
 *   - Strip "<Field>: Not discussed" filler lines
 *   - Strip leading `-`, `•`, `*` bullets
 */
export function preprocessSoap(text: string): string {
  return text
    .replace(ICD_RE, '')
    .replace(NOT_DISCUSSED_LINE_RE, '')
    .replace(LEADING_BULLET_RE, '')
    .replace(/\n{3,}/gu, '\n\n')
    .trim();
}
