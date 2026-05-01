/**
 * Parse a multi-line textarea value into a clean array:
 *  - normalizes CRLF to LF
 *  - splits on newlines
 *  - trims each line
 *  - drops empty lines
 *
 * Used by GenerateTab to convert one-item-per-line list textareas
 * into the string[] shape that PatientContext expects.
 */
export function splitLines(text: string): string[] {
  if (!text) return [];
  return text
    .replace(/\r\n/g, '\n')
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}
