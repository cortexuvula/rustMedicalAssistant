//! Audit logger — redacts PHI from log details before they are written.

use crate::phi_redactor::PhiRedactor;

/// Wraps the audit-logging concern with automatic PHI redaction.
pub struct AuditLogger;

impl AuditLogger {
    /// Create a new `AuditLogger`.
    pub fn new() -> Self {
        Self
    }

    /// Redact any PHI/PII found in `details` so it is safe to log.
    pub fn redact_for_log(details: &str) -> String {
        PhiRedactor::redact(details)
    }
}

impl Default for AuditLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_phi_in_log_details() {
        let input = "User action: looked up SSN 123-45-6789 for patient john@example.com";
        let output = AuditLogger::redact_for_log(input);
        assert!(!output.contains("123-45-6789"), "SSN should be redacted: {}", output);
        assert!(!output.contains("john@example.com"), "email should be redacted: {}", output);
        assert!(output.contains("[SSN]"), "expected [SSN] placeholder: {}", output);
        assert!(output.contains("[EMAIL]"), "expected [EMAIL] placeholder: {}", output);
    }
}
