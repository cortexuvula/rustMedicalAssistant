use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VocabularyCategory {
    DoctorNames,
    MedicationNames,
    MedicalTerminology,
    Abbreviations,
    General,
}

impl Default for VocabularyCategory {
    fn default() -> Self {
        Self::General
    }
}

impl VocabularyCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DoctorNames => "doctor_names",
            Self::MedicationNames => "medication_names",
            Self::MedicalTerminology => "medical_terminology",
            Self::Abbreviations => "abbreviations",
            Self::General => "general",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "doctor_names" => Self::DoctorNames,
            "medication_names" => Self::MedicationNames,
            "medical_terminology" => Self::MedicalTerminology,
            "abbreviations" => Self::Abbreviations,
            _ => Self::General,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabularyEntry {
    pub id: Uuid,
    pub find_text: String,
    pub replacement: String,
    pub category: VocabularyCategory,
    pub case_sensitive: bool,
    pub priority: i32,
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppliedCorrection {
    pub find_text: String,
    pub replacement: String,
    pub category: VocabularyCategory,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrectionResult {
    pub original_text: String,
    pub corrected_text: String,
    pub corrections_applied: Vec<AppliedCorrection>,
    pub total_replacements: u32,
}
