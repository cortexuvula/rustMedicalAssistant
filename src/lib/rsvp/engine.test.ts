import { describe, it, expect } from 'vitest';
import { orpIndex } from './engine';

describe('orpIndex', () => {
  it('returns 0 for words of length 1-3', () => {
    expect(orpIndex('I')).toBe(0);
    expect(orpIndex('to')).toBe(0);
    expect(orpIndex('the')).toBe(0);
  });

  it('returns 1 for words of length 4-5', () => {
    expect(orpIndex('hope')).toBe(1);
    expect(orpIndex('heart')).toBe(1);
  });

  it('returns 2 for words of length 6-9', () => {
    expect(orpIndex('doctor')).toBe(2);
    expect(orpIndex('medicine')).toBe(2);
    expect(orpIndex('prescribe')).toBe(2);
  });

  it('returns 3 for words of length 10+', () => {
    expect(orpIndex('hypertension')).toBe(3);
    expect(orpIndex('cardiovascular')).toBe(3);
  });

  it('ignores trailing punctuation', () => {
    expect(orpIndex('hope.')).toBe(1);
    expect(orpIndex('doctor,')).toBe(2);
    expect(orpIndex('well!')).toBe(1);
  });

  it('returns 0 for empty string', () => {
    expect(orpIndex('')).toBe(0);
  });
});
