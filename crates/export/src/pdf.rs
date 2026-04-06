use medical_core::types::recording::Recording;
use printpdf::*;

use crate::{ExportError, ExportResult};

// ── Exporter ─────────────────────────────────────────────────────────────────

pub struct PdfExporter;

impl PdfExporter {
    /// Exports the SOAP note from a recording as a PDF document.
    pub fn export_soap(recording: &Recording) -> ExportResult<Vec<u8>> {
        let soap = recording.soap_note.as_deref().ok_or_else(|| {
            ExportError::Pdf("Recording has no SOAP note".to_string())
        })?;
        let date = recording.created_at.format("%Y-%m-%d").to_string();
        render_document("SOAP Note", soap, &date)
    }

    /// Exports the referral letter from a recording as a PDF document.
    pub fn export_referral(recording: &Recording) -> ExportResult<Vec<u8>> {
        let referral = recording.referral.as_deref().ok_or_else(|| {
            ExportError::Pdf("Recording has no referral letter".to_string())
        })?;
        let date = recording.created_at.format("%Y-%m-%d").to_string();
        render_document("Referral Letter", referral, &date)
    }

    /// Exports the general letter from a recording as a PDF document.
    pub fn export_letter(recording: &Recording) -> ExportResult<Vec<u8>> {
        let letter = recording.letter.as_deref().ok_or_else(|| {
            ExportError::Pdf("Recording has no letter".to_string())
        })?;
        let date = recording.created_at.format("%Y-%m-%d").to_string();
        render_document("Letter", letter, &date)
    }
}

// ── Renderer ─────────────────────────────────────────────────────────────────

/// SOAP section header prefixes that should be rendered in bold.
const SOAP_HEADERS: &[&str] = &["S:", "O:", "A:", "P:"];

/// Renders a PDF with an A4 page.
///
/// Layout:
///   - Title (16pt, HelveticaBold) at top
///   - Date (10pt, Helvetica) below the title
///   - Body lines rendered with SOAP section headers (S:/O:/A:/P:) in bold
pub fn render_document(title: &str, body: &str, date: &str) -> ExportResult<Vec<u8>> {
    const A4_WIDTH: f32 = 210.0;
    const A4_HEIGHT: f32 = 297.0;
    const MARGIN_LEFT: f32 = 15.0;
    const MARGIN_TOP: f32 = 280.0;
    const LINE_HEIGHT: f32 = 6.0;

    let (doc, page1, layer1) =
        PdfDocument::new(title, Mm(A4_WIDTH), Mm(A4_HEIGHT), "Main Layer");
    let current_layer = doc.get_page(page1).get_layer(layer1);

    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| ExportError::Pdf(format!("Font load error: {e}")))?;
    let font_bold = doc
        .add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| ExportError::Pdf(format!("Bold font load error: {e}")))?;

    let mut y = MARGIN_TOP;

    // Title
    current_layer.use_text(title, 16.0, Mm(MARGIN_LEFT), Mm(y), &font_bold);
    y -= LINE_HEIGHT * 1.5;

    // Date
    current_layer.use_text(date, 10.0, Mm(MARGIN_LEFT), Mm(y), &font);
    y -= LINE_HEIGHT * 2.0;

    // Body — line by line
    for line in body.lines() {
        if y < 10.0 {
            // Page overflow guard — skip remaining lines rather than panic.
            break;
        }
        let is_header = SOAP_HEADERS.iter().any(|&h| line.starts_with(h));
        if is_header {
            current_layer.use_text(line, 11.0, Mm(MARGIN_LEFT), Mm(y), &font_bold);
        } else {
            current_layer.use_text(line, 10.0, Mm(MARGIN_LEFT), Mm(y), &font);
        }
        y -= LINE_HEIGHT;
    }

    let mut buf: Vec<u8> = Vec::new();
    doc.save(&mut std::io::BufWriter::new(&mut buf))
        .map_err(|e| ExportError::Pdf(format!("PDF save error: {e}")))?;

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
    fn export_soap_produces_pdf() {
        let recording = recording_with_soap();
        let bytes = PdfExporter::export_soap(&recording).expect("export OK");
        assert!(!bytes.is_empty());
        // PDF files start with the %PDF- magic bytes
        assert!(bytes.starts_with(b"%PDF-"), "not a valid PDF");
    }

    #[test]
    fn export_without_note_errors() {
        let recording = Recording::new("empty.wav", PathBuf::from("/tmp/empty.wav"));
        let result = PdfExporter::export_soap(&recording);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("SOAP note"));
    }

    #[test]
    fn export_referral_without_referral_errors() {
        let recording = Recording::new("empty.wav", PathBuf::from("/tmp/empty.wav"));
        let result = PdfExporter::export_referral(&recording);
        assert!(result.is_err());
    }

    #[test]
    fn export_letter_without_letter_errors() {
        let recording = Recording::new("empty.wav", PathBuf::from("/tmp/empty.wav"));
        let result = PdfExporter::export_letter(&recording);
        assert!(result.is_err());
    }
}
