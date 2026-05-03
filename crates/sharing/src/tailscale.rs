//! Parser for `tailscale status --json` output. Extracted into a pure
//! function so we can fixture-test it without running the binary.

use serde_json::Value;

/// Given the bytes of a `tailscale status --json` output, return the
/// `Self.DNSName` (with any trailing dot stripped) if present.
///
/// Returns `None` for malformed JSON, missing `Self`, or missing DNS
/// name.
pub fn parse_self_dns_name(json: &[u8]) -> Option<String> {
    let v: Value = serde_json::from_slice(json).ok()?;
    let dns = v.get("Self")?.get("DNSName")?.as_str()?;
    Some(dns.trim_end_matches('.').to_string())
}
