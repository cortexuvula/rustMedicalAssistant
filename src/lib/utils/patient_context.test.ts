import { describe, expect, it } from 'vitest';
import { buildPatientContext } from './patient_context';

describe('buildPatientContext', () => {
  it('returns undefined when all three textareas are empty', () => {
    expect(buildPatientContext('', '', '')).toBeUndefined();
    expect(buildPatientContext('   ', '\n', '\r\n')).toBeUndefined();
  });

  it('returns a populated PatientContext when medications has content', () => {
    const result = buildPatientContext('Lisinopril 10mg', '', '');
    expect(result).toEqual({
      patient_name: null,
      prior_soap_notes: [],
      medications: ['Lisinopril 10mg'],
      allergies: [],
      conditions: [],
    });
  });

  it('returns a populated PatientContext when allergies has content', () => {
    const result = buildPatientContext('', 'Penicillin', '');
    expect(result?.allergies).toEqual(['Penicillin']);
    expect(result?.medications).toEqual([]);
    expect(result?.conditions).toEqual([]);
  });

  it('returns a populated PatientContext when conditions has content', () => {
    const result = buildPatientContext('', '', 'Type 2 diabetes');
    expect(result?.conditions).toEqual(['Type 2 diabetes']);
  });

  it('handles all three populated', () => {
    const result = buildPatientContext(
      'Lisinopril 10mg\nMetformin 500mg BID',
      'Penicillin (rash)',
      'HTN\nT2DM',
    );
    expect(result).toEqual({
      patient_name: null,
      prior_soap_notes: [],
      medications: ['Lisinopril 10mg', 'Metformin 500mg BID'],
      allergies: ['Penicillin (rash)'],
      conditions: ['HTN', 'T2DM'],
    });
  });

  it('normalizes whitespace, blank lines, and CRLF via splitLines', () => {
    const result = buildPatientContext(
      '  a\r\nb  \n\n  c  ',
      '',
      '',
    );
    expect(result?.medications).toEqual(['a', 'b', 'c']);
  });
});
