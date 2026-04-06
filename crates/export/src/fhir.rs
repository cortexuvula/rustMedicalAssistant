use base64::Engine;
use chrono::Utc;
use medical_core::types::recording::Recording;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{ExportError, ExportResult};

// ── Data structures ──────────────────────────────────────────────────────────

/// A FHIR R4 Bundle document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FhirBundle {
    #[serde(rename = "resourceType")]
    pub resource_type: String,
    pub id: String,
    #[serde(rename = "type")]
    pub bundle_type: String,
    pub timestamp: String,
    pub entry: Vec<BundleEntry>,
}

/// A single entry inside a FHIR Bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleEntry {
    pub resource: Value,
}

/// Optional patient demographic information for the bundle.
#[derive(Debug, Clone, Default)]
pub struct PatientInfo {
    pub name: Option<String>,
    pub birth_date: Option<String>,
    pub gender: Option<String>,
    pub identifier: Option<String>,
}

/// Optional practitioner information for the bundle.
#[derive(Debug, Clone, Default)]
pub struct PractitionerInfo {
    pub name: Option<String>,
    pub identifier: Option<String>,
    pub specialty: Option<String>,
}

// ── Helper ───────────────────────────────────────────────────────────────────

/// Base64-encodes a UTF-8 string using the standard alphabet.
pub fn base64_encode(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
}

// ── Exporter ─────────────────────────────────────────────────────────────────

pub struct FhirExporter;

impl FhirExporter {
    /// Builds a full FHIR R4 Bundle from a recording.
    ///
    /// The bundle always contains Patient, Practitioner, and Encounter resources.
    /// If the recording has a SOAP note a DocumentReference (LOINC 11506-3) is added.
    /// If the recording has a transcript a DocumentReference (LOINC 11488-4) is added.
    pub fn export_bundle(
        recording: &Recording,
        patient: PatientInfo,
        practitioner: PractitionerInfo,
    ) -> ExportResult<Vec<u8>> {
        let now = Utc::now().to_rfc3339();
        let bundle_id = Uuid::new_v4().to_string();

        let patient_id = Uuid::new_v4().to_string();
        let practitioner_id = Uuid::new_v4().to_string();
        let encounter_id = Uuid::new_v4().to_string();

        let mut entries: Vec<BundleEntry> = Vec::new();

        // ── Patient ──────────────────────────────────────────────────────────
        let patient_name = patient
            .name
            .clone()
            .or_else(|| recording.patient_name.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        let mut patient_resource = json!({
            "resourceType": "Patient",
            "id": patient_id,
            "name": [{ "text": patient_name }]
        });

        if let Some(bd) = &patient.birth_date {
            patient_resource["birthDate"] = json!(bd);
        }
        if let Some(g) = &patient.gender {
            patient_resource["gender"] = json!(g);
        }
        if let Some(ident) = &patient.identifier {
            patient_resource["identifier"] = json!([{ "value": ident }]);
        }

        entries.push(BundleEntry { resource: patient_resource });

        // ── Practitioner ─────────────────────────────────────────────────────
        let prac_name = practitioner.name.clone().unwrap_or_else(|| "Unknown".to_string());
        let mut prac_resource = json!({
            "resourceType": "Practitioner",
            "id": practitioner_id,
            "name": [{ "text": prac_name }]
        });

        if let Some(ident) = &practitioner.identifier {
            prac_resource["identifier"] = json!([{ "value": ident }]);
        }
        if let Some(spec) = &practitioner.specialty {
            prac_resource["qualification"] = json!([{
                "code": { "text": spec }
            }]);
        }

        entries.push(BundleEntry { resource: prac_resource });

        // ── Encounter ────────────────────────────────────────────────────────
        let encounter_resource = json!({
            "resourceType": "Encounter",
            "id": encounter_id,
            "status": "finished",
            "class": {
                "system": "http://terminology.hl7.org/CodeSystem/v3-ActCode",
                "code": "AMB",
                "display": "ambulatory"
            },
            "subject": { "reference": format!("Patient/{}", patient_id) },
            "participant": [{
                "individual": { "reference": format!("Practitioner/{}", practitioner_id) }
            }],
            "period": { "start": recording.created_at.to_rfc3339() }
        });
        entries.push(BundleEntry { resource: encounter_resource });

        // ── SOAP DocumentReference (LOINC 11506-3) ───────────────────────────
        if let Some(soap) = &recording.soap_note {
            let doc_ref = Self::build_document_reference(
                &patient_id,
                "Progress note",
                "11506-3",
                soap,
                &recording.created_at.to_rfc3339(),
            );
            entries.push(BundleEntry { resource: doc_ref });
        }

        // ── Transcript DocumentReference (LOINC 11488-4) ────────────────────
        if let Some(transcript) = &recording.transcript {
            let doc_ref = Self::build_document_reference(
                &patient_id,
                "Consultation note",
                "11488-4",
                transcript,
                &recording.created_at.to_rfc3339(),
            );
            entries.push(BundleEntry { resource: doc_ref });
        }

        // ── Bundle ───────────────────────────────────────────────────────────
        let bundle = FhirBundle {
            resource_type: "Bundle".to_string(),
            id: bundle_id,
            bundle_type: "document".to_string(),
            timestamp: now,
            entry: entries,
        };

        serde_json::to_vec_pretty(&bundle)
            .map_err(|e| ExportError::Fhir(format!("JSON serialization failed: {e}")))
    }

    /// Exports a standalone FHIR DocumentReference for the recording.
    pub fn export_document_reference(recording: &Recording, title: &str) -> ExportResult<Vec<u8>> {
        let content = recording
            .soap_note
            .as_deref()
            .or(recording.transcript.as_deref())
            .unwrap_or("");

        let doc_ref = Self::build_document_reference(
            &Uuid::new_v4().to_string(),
            title,
            "11506-3",
            content,
            &recording.created_at.to_rfc3339(),
        );

        serde_json::to_vec_pretty(&doc_ref)
            .map_err(|e| ExportError::Fhir(format!("JSON serialization failed: {e}")))
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn build_document_reference(
        patient_id: &str,
        title: &str,
        loinc_code: &str,
        content: &str,
        date: &str,
    ) -> Value {
        let encoded = base64_encode(content);
        json!({
            "resourceType": "DocumentReference",
            "id": Uuid::new_v4().to_string(),
            "status": "current",
            "type": {
                "coding": [{
                    "system": "http://loinc.org",
                    "code": loinc_code,
                    "display": title
                }]
            },
            "subject": { "reference": format!("Patient/{}", patient_id) },
            "date": date,
            "content": [{
                "attachment": {
                    "contentType": "text/plain",
                    "data": encoded,
                    "title": title
                }
            }]
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use medical_core::types::recording::Recording;

    fn make_recording() -> Recording {
        let mut r = Recording::new("test.wav", PathBuf::from("/tmp/test.wav"));
        r.soap_note = Some("S: patient complains\nO: normal\nA: healthy\nP: rest".to_string());
        r.transcript = Some("Doctor: how are you? Patient: fine.".to_string());
        r.patient_name = Some("Jane Doe".to_string());
        r
    }

    #[test]
    fn export_bundle_valid_json() {
        let recording = make_recording();
        let bytes = FhirExporter::export_bundle(
            &recording,
            PatientInfo::default(),
            PractitionerInfo::default(),
        )
        .unwrap();

        let json: Value = serde_json::from_slice(&bytes).expect("valid JSON");
        assert_eq!(json["resourceType"], "Bundle");
        assert_eq!(json["type"], "document");
    }

    #[test]
    fn contains_patient_resource() {
        let recording = make_recording();
        let bytes = FhirExporter::export_bundle(
            &recording,
            PatientInfo::default(),
            PractitionerInfo::default(),
        )
        .unwrap();

        let bundle: FhirBundle = serde_json::from_slice(&bytes).unwrap();
        let has_patient = bundle
            .entry
            .iter()
            .any(|e| e.resource["resourceType"] == "Patient");
        assert!(has_patient);
    }

    #[test]
    fn contains_soap_doc_ref() {
        let recording = make_recording();
        let bytes = FhirExporter::export_bundle(
            &recording,
            PatientInfo::default(),
            PractitionerInfo::default(),
        )
        .unwrap();

        let bundle: FhirBundle = serde_json::from_slice(&bytes).unwrap();
        let has_doc_ref = bundle.entry.iter().any(|e| {
            e.resource["resourceType"] == "DocumentReference"
                && e.resource["type"]["coding"][0]["code"] == "11506-3"
        });
        assert!(has_doc_ref);
    }

    #[test]
    fn export_doc_reference() {
        let recording = make_recording();
        let bytes = FhirExporter::export_document_reference(&recording, "Progress note").unwrap();
        let json: Value = serde_json::from_slice(&bytes).expect("valid JSON");
        assert_eq!(json["resourceType"], "DocumentReference");
        assert_eq!(json["status"], "current");
    }

    #[test]
    fn recording_without_soap_still_exports() {
        let mut recording = Recording::new("empty.wav", PathBuf::from("/tmp/empty.wav"));
        recording.transcript = Some("Some transcript".to_string());
        // No soap_note set

        let bytes = FhirExporter::export_bundle(
            &recording,
            PatientInfo::default(),
            PractitionerInfo::default(),
        )
        .unwrap();

        let bundle: FhirBundle = serde_json::from_slice(&bytes).unwrap();

        // Should have Patient, Practitioner, Encounter (no SOAP doc ref)
        let soap_doc_ref = bundle.entry.iter().any(|e| {
            e.resource["resourceType"] == "DocumentReference"
                && e.resource["type"]["coding"][0]["code"] == "11506-3"
        });
        assert!(!soap_doc_ref, "SOAP doc ref should not be present");

        // But transcript doc ref should be present
        let transcript_doc_ref = bundle.entry.iter().any(|e| {
            e.resource["resourceType"] == "DocumentReference"
                && e.resource["type"]["coding"][0]["code"] == "11488-4"
        });
        assert!(transcript_doc_ref, "Transcript doc ref should be present");
    }
}
