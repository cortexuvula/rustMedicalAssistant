import { describe, it, expect } from 'vitest';
import { orpIndex } from './engine';
import { baseDelayMs, delayMs, type Token } from './engine';
import { tokenize } from './engine';

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

describe('baseDelayMs', () => {
  it('returns 200 for 300 WPM', () => {
    expect(baseDelayMs(300)).toBe(200);
  });
  it('returns 600 for 100 WPM', () => {
    expect(baseDelayMs(100)).toBe(600);
  });
  it('returns 100 for 600 WPM', () => {
    expect(baseDelayMs(600)).toBe(100);
  });
});

describe('delayMs', () => {
  const base = 200;
  it('word: 1.0x', () => {
    const t: Token = { word: 'patient', kind: 'word' };
    expect(delayMs(t, base)).toBe(200);
  });
  it('clause: 1.5x', () => {
    const t: Token = { word: 'patient,', kind: 'clause' };
    expect(delayMs(t, base)).toBe(300);
  });
  it('sentence: 2.5x', () => {
    const t: Token = { word: 'fine.', kind: 'sentence' };
    expect(delayMs(t, base)).toBe(500);
  });
  it('header: 3.0x', () => {
    const t: Token = { word: 'Subjective:', kind: 'header' };
    expect(delayMs(t, base)).toBe(600);
  });
});

describe('tokenize', () => {
  it('classifies plain words', () => {
    const t = tokenize('the patient reports');
    expect(t.map((x) => x.kind)).toEqual(['word', 'word', 'word']);
    expect(t.map((x) => x.word)).toEqual(['the', 'patient', 'reports']);
  });

  it('classifies clause punctuation', () => {
    const t = tokenize('feels well, no complaints');
    expect(t[1]).toEqual({ word: 'well,', kind: 'clause' });
  });

  it('classifies sentence terminators', () => {
    const t = tokenize('He is fine. She is unwell!');
    expect(t[2]).toEqual({ word: 'fine.', kind: 'sentence' });
    expect(t[5]).toEqual({ word: 'unwell!', kind: 'sentence' });
  });

  it('classifies SOAP headers as header kind', () => {
    const t = tokenize('Subjective: the patient reports');
    expect(t[0]).toEqual({ word: 'Subjective:', kind: 'header' });
  });

  it('treats markdown-bold headers as headers', () => {
    const t = tokenize('**Objective:** vitals stable');
    expect(t[0].kind).toBe('header');
  });

  it('does not classify non-SOAP trailing-colon words as header', () => {
    const t = tokenize('Time: 09:30 today');
    expect(t[0].kind).not.toBe('header');
  });

  it('preserves order and word count', () => {
    const t = tokenize('one two three four.');
    expect(t.length).toBe(4);
    expect(t[3]).toEqual({ word: 'four.', kind: 'sentence' });
  });
});
