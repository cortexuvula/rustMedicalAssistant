use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// A recorded consultation with optional transcript and generated documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recording {
    pub id: Uuid,
    pub filename: String,
    pub transcript: Option<String>,
    pub soap_note: Option<String>,
    pub referral: Option<String>,
    pub letter: Option<String>,
    pub chat: Option<String>,
    pub patient_name: Option<String>,
    pub audio_path: PathBuf,
    pub duration_seconds: Option<f64>,
    pub file_size_bytes: Option<u64>,
    pub stt_provider: Option<String>,
    pub ai_provider: Option<String>,
    pub tags: Vec<String>,
    pub status: ProcessingStatus,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

impl Recording {
    /// Create a new recording in the Pending state.
    pub fn new(filename: impl Into<String>, audio_path: PathBuf) -> Self {
        Self {
            id: Uuid::new_v4(),
            filename: filename.into(),
            transcript: None,
            soap_note: None,
            referral: None,
            letter: None,
            chat: None,
            patient_name: None,
            audio_path,
            duration_seconds: None,
            file_size_bytes: None,
            stt_provider: None,
            ai_provider: None,
            tags: Vec::new(),
            status: ProcessingStatus::Pending,
            created_at: Utc::now(),
            metadata: serde_json::Value::Null,
        }
    }

    /// Returns true if processing has completed successfully.
    pub fn is_processed(&self) -> bool {
        matches!(self.status, ProcessingStatus::Completed { .. })
    }

    /// Returns true if a transcript is present.
    pub fn has_transcript(&self) -> bool {
        self.transcript.is_some()
    }

    /// Returns true if a SOAP note is present.
    pub fn has_soap_note(&self) -> bool {
        self.soap_note.is_some()
    }
}

/// Processing lifecycle of a recording.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProcessingStatus {
    Pending,
    Processing {
        started_at: DateTime<Utc>,
    },
    Completed {
        completed_at: DateTime<Utc>,
    },
    Failed {
        error: String,
        retry_count: u32,
    },
}

impl ProcessingStatus {
    /// Returns true if no further automatic transitions are expected.
    pub fn is_terminal(&self) -> bool {
        match self {
            ProcessingStatus::Completed { .. } => true,
            ProcessingStatus::Failed { retry_count, .. } => *retry_count >= 3,
            _ => false,
        }
    }

    /// Returns true if the task can be retried (failed fewer than 3 times).
    pub fn can_retry(&self) -> bool {
        match self {
            ProcessingStatus::Failed { retry_count, .. } => *retry_count < 3,
            _ => false,
        }
    }

    /// A human-readable label for the status.
    pub fn status_label(&self) -> &'static str {
        match self {
            ProcessingStatus::Pending => "Pending",
            ProcessingStatus::Processing { .. } => "Processing",
            ProcessingStatus::Completed { .. } => "Completed",
            ProcessingStatus::Failed { .. } => "Failed",
        }
    }
}

/// Lightweight summary of a recording suitable for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingSummary {
    pub id: Uuid,
    pub filename: String,
    pub patient_name: Option<String>,
    pub status: ProcessingStatus,
    pub duration_seconds: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub tags: Vec<String>,
    pub has_transcript: bool,
    pub has_soap_note: bool,
    pub has_referral: bool,
    pub has_letter: bool,
}

impl From<&Recording> for RecordingSummary {
    fn from(r: &Recording) -> Self {
        Self {
            id: r.id,
            filename: r.filename.clone(),
            patient_name: r.patient_name.clone(),
            status: r.status.clone(),
            duration_seconds: r.duration_seconds,
            created_at: r.created_at,
            tags: r.tags.clone(),
            has_transcript: r.transcript.is_some(),
            has_soap_note: r.soap_note.is_some(),
            has_referral: r.referral.is_some(),
            has_letter: r.letter.is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_recording_starts_pending() {
        let rec = Recording::new("test.wav", PathBuf::from("/audio/test.wav"));
        assert!(matches!(rec.status, ProcessingStatus::Pending));
        assert!(!rec.is_processed());
        assert!(!rec.has_transcript());
        assert!(!rec.has_soap_note());
    }

    #[test]
    fn processing_status_terminal_states() {
        let completed = ProcessingStatus::Completed {
            completed_at: Utc::now(),
        };
        assert!(completed.is_terminal());

        let failed_max = ProcessingStatus::Failed {
            error: "boom".into(),
            retry_count: 3,
        };
        assert!(failed_max.is_terminal());

        let failed_once = ProcessingStatus::Failed {
            error: "boom".into(),
            retry_count: 1,
        };
        assert!(!failed_once.is_terminal());

        let pending = ProcessingStatus::Pending;
        assert!(!pending.is_terminal());
    }

    #[test]
    fn retry_logic() {
        let retryable = ProcessingStatus::Failed {
            error: "err".into(),
            retry_count: 2,
        };
        assert!(retryable.can_retry());

        let exhausted = ProcessingStatus::Failed {
            error: "err".into(),
            retry_count: 3,
        };
        assert!(!exhausted.can_retry());

        assert!(!ProcessingStatus::Pending.can_retry());
        assert!(!ProcessingStatus::Completed { completed_at: Utc::now() }.can_retry());
    }

    #[test]
    fn serializes_with_tag() {
        let status = ProcessingStatus::Pending;
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["status"], "pending");

        let processing = ProcessingStatus::Processing {
            started_at: Utc::now(),
        };
        let json = serde_json::to_value(&processing).unwrap();
        assert_eq!(json["status"], "processing");
        assert!(json["started_at"].is_string());

        let failed = ProcessingStatus::Failed {
            error: "oops".into(),
            retry_count: 1,
        };
        let json = serde_json::to_value(&failed).unwrap();
        assert_eq!(json["status"], "failed");
        assert_eq!(json["error"], "oops");
        assert_eq!(json["retry_count"], 1);
    }

    #[test]
    fn summary_from_recording() {
        let mut rec = Recording::new("visit.wav", PathBuf::from("/audio/visit.wav"));
        rec.transcript = Some("Hello".into());
        rec.soap_note = Some("S: ....".into());
        rec.patient_name = Some("Jane Doe".into());

        let summary = RecordingSummary::from(&rec);
        assert_eq!(summary.filename, "visit.wav");
        assert!(summary.has_transcript);
        assert!(summary.has_soap_note);
        assert!(!summary.has_referral);
        assert!(!summary.has_letter);
        assert_eq!(summary.patient_name.as_deref(), Some("Jane Doe"));
    }

    #[test]
    fn status_labels() {
        assert_eq!(ProcessingStatus::Pending.status_label(), "Pending");
        assert_eq!(
            ProcessingStatus::Processing { started_at: Utc::now() }.status_label(),
            "Processing"
        );
        assert_eq!(
            ProcessingStatus::Completed { completed_at: Utc::now() }.status_label(),
            "Completed"
        );
        assert_eq!(
            ProcessingStatus::Failed { error: "e".into(), retry_count: 0 }.status_label(),
            "Failed"
        );
    }
}
