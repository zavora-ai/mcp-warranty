//! Validate mcp-server.toml parses, passes SDK validation, has the right tool
//! count, and gates the adjudication + RMA writes.

use adk_mcp_sdk::manifest::ServerManifest;
use std::path::Path;

fn manifest() -> ServerManifest {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("mcp-server.toml");
    ServerManifest::from_file(&path).expect("manifest should parse")
}

#[test]
fn manifest_parses_and_validates() {
    let m = manifest();
    assert!(m.validate().is_empty(), "validation errors: {:?}", m.validate());
    assert_eq!(m.server_id, "mcp_warranty");
    assert_eq!(m.domain, "warranty");
    assert_eq!(m.tools.len(), 24, "expected 24 declared tools");
}

#[test]
fn financial_writes_are_gated_external() {
    use adk_mcp_sdk::risk::RiskClass;
    let m = manifest();
    for name in ["approve_claim", "reject_claim", "issue_rma"] {
        let t = m.tools.iter().find(|t| t.name == name).unwrap_or_else(|| panic!("{name} present"));
        assert!(t.requires_approval, "{name} must require approval");
        assert_eq!(t.risk_class, RiskClass::ExternalWrite, "{name} must be external_write");
    }
}

#[test]
fn validate_claim_is_read_only() {
    use adk_mcp_sdk::risk::RiskClass;
    let m = manifest();
    // The adjudication pre-check must be a pure read.
    let t = m.tools.iter().find(|t| t.name == "validate_claim").unwrap();
    assert_eq!(t.risk_class, RiskClass::ReadOnly);
    assert!(!t.requires_approval);
}

#[test]
fn reads_are_read_only() {
    use adk_mcp_sdk::risk::RiskClass;
    let m = manifest();
    for name in ["get_product", "list_plans", "get_registration", "check_entitlement", "get_claim", "list_claims", "claims_analytics", "get_rma", "audit_log"] {
        let t = m.tools.iter().find(|t| t.name == name).unwrap();
        assert_eq!(t.risk_class, RiskClass::ReadOnly, "{name} should be read_only");
    }
}
