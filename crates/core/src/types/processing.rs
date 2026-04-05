use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Relative priority for queue tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum Priority {
    Low,
    #[default]
    Normal,
    High,
}


impl Priority {
    /// Returns a signed integer representation suitable for ordering queries.
    pub fn as_i32(self) -> i32 {
        match self {
            Priority::Low => -1,
            Priority::Normal => 0,
            Priority::High => 1,
        }
    }
}

/// The kind of work a queue task represents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    Transcribe,
    GenerateSoap,
    GenerateReferral,
    GenerateLetter,
    ExtractData,
    IndexRag,
}

/// A single unit of work in the processing queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueTask {
    pub id: Uuid,
    pub recording_id: Uuid,
    pub task_type: TaskType,
    pub priority: Priority,
    pub status: QueueTaskStatus,
    pub created_at: DateTime<Utc>,
    pub batch_id: Option<Uuid>,
}

/// Lifecycle state of a queue task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum QueueTaskStatus {
    Pending,
    Processing {
        started_at: DateTime<Utc>,
    },
    Completed {
        completed_at: DateTime<Utc>,
        result: Option<String>,
    },
    Failed {
        error: String,
        error_count: u32,
    },
}

/// Options controlling how a batch processing job behaves.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchProcessingOptions {
    pub generate_soap: bool,
    pub generate_referral: bool,
    pub generate_letter: bool,
    pub skip_existing: bool,
    pub continue_on_error: bool,
    pub priority: Priority,
    pub max_concurrent: u32,
}

impl Default for BatchProcessingOptions {
    fn default() -> Self {
        Self {
            generate_soap: true,
            generate_referral: false,
            generate_letter: false,
            skip_existing: true,
            continue_on_error: true,
            priority: Priority::Normal,
            max_concurrent: 3,
        }
    }
}

/// Overall status of a batch job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchStatus {
    pub batch_id: Uuid,
    pub state: BatchState,
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
    pub created_at: DateTime<Utc>,
}

/// High-level lifecycle state of a batch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchState {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Events emitted during processing for progress tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ProcessingEvent {
    TaskQueued {
        task_id: Uuid,
        recording_id: Uuid,
        task_type: TaskType,
    },
    TaskStarted {
        task_id: Uuid,
        recording_id: Uuid,
    },
    TaskCompleted {
        task_id: Uuid,
        recording_id: Uuid,
        duration_ms: u64,
    },
    TaskFailed {
        task_id: Uuid,
        recording_id: Uuid,
        error: String,
    },
    BatchCompleted {
        batch_id: Uuid,
        total: u32,
        failed: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering() {
        assert!(Priority::High > Priority::Normal);
        assert!(Priority::Normal > Priority::Low);
        assert_eq!(Priority::Low.as_i32(), -1);
        assert_eq!(Priority::Normal.as_i32(), 0);
        assert_eq!(Priority::High.as_i32(), 1);
        assert_eq!(Priority::default(), Priority::Normal);
    }

    #[test]
    fn batch_options_defaults() {
        let opts = BatchProcessingOptions::default();
        assert!(opts.generate_soap);
        assert!(!opts.generate_referral);
        assert!(!opts.generate_letter);
        assert!(opts.skip_existing);
        assert!(opts.continue_on_error);
        assert_eq!(opts.priority, Priority::Normal);
        assert_eq!(opts.max_concurrent, 3);
    }

    #[test]
    fn queue_task_status_serializes() {
        let status = QueueTaskStatus::Pending;
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["status"], "pending");

        let failed = QueueTaskStatus::Failed {
            error: "network".into(),
            error_count: 2,
        };
        let json = serde_json::to_value(&failed).unwrap();
        assert_eq!(json["status"], "failed");
        assert_eq!(json["error_count"], 2);

        let completed = QueueTaskStatus::Completed {
            completed_at: Utc::now(),
            result: Some("done".into()),
        };
        let json = serde_json::to_value(&completed).unwrap();
        assert_eq!(json["status"], "completed");
        assert_eq!(json["result"], "done");
    }

    #[test]
    fn processing_event_serializes() {
        let id = Uuid::new_v4();
        let rec_id = Uuid::new_v4();
        let event = ProcessingEvent::TaskStarted {
            task_id: id,
            recording_id: rec_id,
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "task_started");

        let batch_event = ProcessingEvent::BatchCompleted {
            batch_id: Uuid::new_v4(),
            total: 10,
            failed: 1,
        };
        let json = serde_json::to_value(&batch_event).unwrap();
        assert_eq!(json["event"], "batch_completed");
        assert_eq!(json["total"], 10);
    }
}
