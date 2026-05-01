import { describe, expect, it } from 'vitest';
import { splitLines } from './text';

describe('splitLines', () => {
  it('returns an empty array for empty input', () => {
    expect(splitLines('')).toEqual([]);
    expect(splitLines('   ')).toEqual([]);
    expect(splitLines('\n\n')).toEqual([]);
  });

  it('splits on newlines and trims each line', () => {
    expect(splitLines('  a\nb  \n  c  ')).toEqual(['a', 'b', 'c']);
  });

  it('drops blank lines', () => {
    expect(splitLines('a\n\nb\n   \nc')).toEqual(['a', 'b', 'c']);
  });

  it('normalizes CRLF to LF before splitting', () => {
    expect(splitLines('a\r\nb\r\nc')).toEqual(['a', 'b', 'c']);
  });

  it('preserves internal whitespace within a line', () => {
    expect(splitLines('Lisinopril 10mg PO daily\nMetformin 500mg BID')).toEqual([
      'Lisinopril 10mg PO daily',
      'Metformin 500mg BID',
    ]);
  });
});
