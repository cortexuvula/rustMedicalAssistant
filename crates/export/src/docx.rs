use std::io::Cursor;

use docx_rs::*;
use medical_core::types::recording::Recording;

use crate::{ExportError, ExportResult};

// ── Exporter ─────────────────────────────────────────────────────────────────

pub struct DocxExporter;

impl DocxExporter {
    /// Exports the SOAP note from a recording as a DOCX document.
    pub fn export_soap(recording: &Recording) -> ExportResult<Vec<u8>> {
        let soap = recording.soap_note.as_deref().ok_or_else(|| {
            ExportError::Docx("Recording has no SOAP note".to_string())
        })?;
        let date = recording.created_at.format("%Y-%m-%d").to_string();
        render_document("SOAP Note", soap, &date)
    }

    /// Exports the referral letter from a recording as a DOCX document.
    pub fn export_referral(recording: &Recording) -> ExportResult<Vec<u8>> {
        let referral = recording.referral.as_deref().ok_or_else(|| {
            ExportError::Docx("Recording has no referral letter".to_string())
        })?;
        let date = recording.created_at.format("%Y-%m-%d").to_string();
        render_document("Referral Letter", referral, &date)
    }

    /// Exports the general letter from a recording as a DOCX document.
    pub fn export_letter(recording: &Recording) -> ExportResult<Vec<u8>> {
        let letter = recording.letter.as_deref().ok_or_else(|| {
            ExportError::Docx("Recording has no letter".to_string())
        })?;
        let date = recording.created_at.format("%Y-%m-%d").to_string();
        render_document("Letter", letter, &date)
    }
}

// ── Renderer ─────────────────────────────────────────────────────────────────

/// SOAP section header prefixes that should be rendered in bold.
const SOAP_HEADERS: &[&str] = &["S:", "O:", "A:", "P:"];

/// Renders a DOCX with a title, date, and body.
///
/// Layout:
///   - Title: centred, bold, size 32 (half-points)
///   - Date: right-aligned, gray (#888888), size 20
///   - Body: SOAP section headers (S:/O:/A:/P:) in bold size 24,
///     regular lines at size 22
pub fn render_document(title: &str, body: &str, date: &str) -> ExportResult<Vec<u8>> {
    let mut docx = Docx::new();

    // ── Title ────────────────────────────────────────────────────────────────
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text(title).bold().size(32))
            .align(AlignmentType::Center),
    );

    // ── Date ─────────────────────────────────────────────────────────────────
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(Run::new().add_text(date).size(20).color("888888"))
            .align(AlignmentType::Right),
    );

    // ── Body ─────────────────────────────────────────────────────────────────
    for line in body.lines() {
        let is_header = SOAP_HEADERS.iter().any(|&h| line.starts_with(h));
        let para = if is_header {
            Paragraph::new().add_run(Run::new().add_text(line).bold().size(24))
        } else {
            Paragraph::new().add_run(Run::new().add_text(line).size(22))
        };
        docx = docx.add_paragraph(para);
    }

    // ── Pack to bytes ────────────────────────────────────────────────────────
    let mut buf: Vec<u8> = Vec::new();
    docx.build()
        .pack(Cursor::new(&mut buf))
        .map_err(|e| ExportError::Docx(format!("DOCX pack error: {e}")))?;

    Ok(buf)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use medical_core::types::recording::Recording;

    fn recording_with_soap() -> Recording {
        let mut r = Recording::new("visit.wav", PathBuf::from("/tmp/visit.wav"));
        r.soap_note = Some(
            "S: Patient reports headache\nO: BP 120/80\nA: Tension headache\nP: Ibuprofen 400mg"
                .to_string(),
        );
        r
    }

    #[test]
    fn export_soap_produces_docx() {
        let recording = recording_with_soap();
        let bytes = DocxExporter::export_soap(&recording).expect("export OK");
        assert!(!bytes.is_empty());
        // DOCX files are ZIP archives — they start with the PK magic bytes (0x50 0x4B)
        assert!(
            bytes.starts_with(&[0x50, 0x4B]),
            "not a valid DOCX/ZIP (no PK magic)"
        );
    }

    #[test]
    fn export_without_note_errors() {
        let recording = Recording::new("empty.wav", PathBuf::from("/tmp/empty.wav"));
        let result = DocxExporter::export_soap(&recording);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("SOAP note"));
    }

    #[test]
    fn export_referral_without_referral_errors() {
        let recording = Recording::new("empty.wav", PathBuf::from("/tmp/empty.wav"));
        let result = DocxExporter::export_referral(&recording);
        assert!(result.is_err());
    }

    #[test]
    fn export_letter_without_letter_errors() {
        let recording = Recording::new("empty.wav", PathBuf::from("/tmp/empty.wav"));
        let result = DocxExporter::export_letter(&recording);
        assert!(result.is_err());
    }
}
