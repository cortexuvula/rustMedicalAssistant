use medical_sharing::tailscale::parse_self_dns_name;

#[test]
fn online_with_dns_name_returns_trimmed_dns() {
    let json = br#"
    {
      "Self": {
        "DNSName": "clinic-server.tail-abc123.ts.net.",
        "Online": true
      }
    }
    "#;
    let got = parse_self_dns_name(json);
    assert_eq!(got.as_deref(), Some("clinic-server.tail-abc123.ts.net"));
}

#[test]
fn offline_with_dns_name_still_returns_dns() {
    let json = br#"
    {
      "Self": {
        "DNSName": "laptop.tail-abc123.ts.net.",
        "Online": false
      }
    }
    "#;
    assert_eq!(
        parse_self_dns_name(json).as_deref(),
        Some("laptop.tail-abc123.ts.net")
    );
}

#[test]
fn missing_self_returns_none() {
    let json = br#"{ "Peers": {} }"#;
    assert!(parse_self_dns_name(json).is_none());
}

#[test]
fn missing_dns_name_returns_none() {
    let json = br#"{ "Self": { "Online": true } }"#;
    assert!(parse_self_dns_name(json).is_none());
}

#[test]
fn malformed_json_returns_none() {
    assert!(parse_self_dns_name(b"not json").is_none());
}

#[test]
fn empty_dns_name_returns_empty_string() {
    let json = br#"{ "Self": { "DNSName": "" } }"#;
    assert_eq!(parse_self_dns_name(json).as_deref(), Some(""));
}

#[test]
fn dns_name_without_trailing_dot_unchanged() {
    let json = br#"{ "Self": { "DNSName": "host.tail-x.ts.net" } }"#;
    assert_eq!(
        parse_self_dns_name(json).as_deref(),
        Some("host.tail-x.ts.net")
    );
}
