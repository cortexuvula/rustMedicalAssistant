import type { PatientContext } from '../types';
import { splitLines } from './text';

/**
 * Build a `PatientContext` payload from three line-per-item textarea values.
 * Returns `undefined` when every list is empty so the backend stores nothing
 * and renders no Patient record block.
 */
export function buildPatientContext(
  medicationsText: string,
  allergiesText: string,
  conditionsText: string,
): PatientContext | undefined {
  const medications = splitLines(medicationsText);
  const allergies = splitLines(allergiesText);
  const conditions = splitLines(conditionsText);
  if (medications.length === 0 && allergies.length === 0 && conditions.length === 0) {
    return undefined;
  }
  return {
    patient_name: null,
    prior_soap_notes: [],
    medications,
    allergies,
    conditions,
  };
}
