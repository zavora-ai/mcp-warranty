//! Integration tests: coverage window, entitlement, claim validation +
//! adjudication (deductible/cap/exclusions), RMA flow, transfer rules, analytics.

use chrono::{Duration, Utc};
use mcp_warranty::store::WarrantyStore;
use mcp_warranty::types::*;

fn store() -> WarrantyStore {
    WarrantyStore::new()
}

fn reg_by_serial(s: &WarrantyStore, serial: &str) -> String {
    s.resolve_registration(serial).unwrap().id
}

#[test]
fn seed_loads() {
    let s = store();
    assert!(s.list_products(None).len() >= 2);
    assert!(s.list_plans().len() >= 2);
    assert!(s.list_registrations(None, None).len() >= 3);
}

#[test]
fn coverage_window_computed_from_plan() {
    let s = store();
    let laptop = s.list_products(None).into_iter().find(|p| p.sku == "LAP-15").unwrap();
    let ext = s.list_plans().into_iter().find(|p| p.kind == PlanKind::Extended).unwrap();
    let pd = chrono::NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();
    let r = s.register(&laptop.id, &ext.id, "SN-TEST-1", "dave", pd, "a").unwrap();
    // 36 months from 2026-01-15 -> 2029-01-15
    assert_eq!(r.coverage_end, chrono::NaiveDate::from_ymd_opt(2029, 1, 15).unwrap());
    assert_eq!(r.status, RegistrationStatus::Active);
}

#[test]
fn entitlement_active_vs_expired() {
    let s = store();
    let active = reg_by_serial(&s, "SN-LAP-0001"); // 200 days ago on 3y plan -> active
    let expired = reg_by_serial(&s, "SN-LAP-9999"); // marked expired
    let today = Utc::now().date_naive();
    let a = s.check_entitlement(&active, today, Some("accidental")).unwrap();
    assert_eq!(a["in_coverage_window"], true);
    assert_eq!(a["failure_covered"], true);
    assert_eq!(a["entitled"], true);
    let e = s.check_entitlement(&expired, today, Some("parts")).unwrap();
    assert_eq!(e["entitled"], false);
}

#[test]
fn entitlement_respects_exclusions() {
    let s = store();
    let active = reg_by_serial(&s, "SN-LAP-0001");
    let today = Utc::now().date_naive();
    // "consumables" is excluded on the extended plan
    let r = s.check_entitlement(&active, today, Some("consumables")).unwrap();
    assert_eq!(r["exclusion_hit"], true);
    assert_eq!(r["failure_covered"], false);
}

#[test]
fn validate_and_approve_applies_deductible_and_cap() {
    let s = store();
    let reg = reg_by_serial(&s, "SN-LAP-0001"); // extended: deductible 50, cap 2000
    let claim = s.submit_claim(&reg, "accidental", "cracked screen", 350.0, "alice").unwrap();
    let v = s.validate_claim(&claim.id).unwrap();
    assert_eq!(v["covered"], true);
    // payable = 350 - 50 deductible = 300
    assert_eq!(v["payable_amount"].as_f64().unwrap(), 300.0);
    let approved = s.approve_claim(&claim.id, "adjuster").unwrap();
    assert_eq!(approved.status, ClaimStatus::Approved);
    assert_eq!(approved.approved_amount, Some(300.0));
    // cap accrual
    let after = s.get_registration(&reg).unwrap();
    assert_eq!(after.claimed_to_date, 300.0);
}

#[test]
fn approve_refuses_excluded_failure() {
    let s = store();
    let reg = reg_by_serial(&s, "SN-LAP-0001");
    let claim = s.submit_claim(&reg, "consumables", "battery wear", 80.0, "alice").unwrap();
    let v = s.validate_claim(&claim.id).unwrap();
    assert_eq!(v["covered"], false);
    let err = s.approve_claim(&claim.id, "adjuster").unwrap_err();
    assert!(err.contains("not covered"), "got: {err}");
}

#[test]
fn approve_refuses_expired_registration() {
    let s = store();
    let expired = reg_by_serial(&s, "SN-LAP-9999");
    let claim = s.submit_claim(&expired, "parts", "broken hinge", 120.0, "carol").unwrap();
    assert!(s.approve_claim(&claim.id, "adjuster").is_err());
}

#[test]
fn cap_limits_payable() {
    let s = store();
    let reg = reg_by_serial(&s, "SN-LAP-0001"); // cap 2000, deductible 50
    // a claim bigger than the cap
    let claim = s.submit_claim(&reg, "parts", "mainboard", 5000.0, "alice").unwrap();
    let v = s.validate_claim(&claim.id).unwrap();
    // payable = min(5000-50, remaining cap 2000) = 2000
    assert_eq!(v["payable_amount"].as_f64().unwrap(), 2000.0);
}

#[test]
fn rma_requires_approved_claim_and_flows() {
    let s = store();
    let reg = reg_by_serial(&s, "SN-LAP-0001");
    let claim = s.submit_claim(&reg, "accidental", "screen", 350.0, "alice").unwrap();
    // can't RMA before approval
    assert!(s.issue_rma(&claim.id, "repair", "x", "ops").is_err());
    s.approve_claim(&claim.id, "adjuster").unwrap();
    let rma = s.issue_rma(&claim.id, "replace", "send new unit", "ops").unwrap();
    assert_eq!(rma.status, RmaStatus::Issued);
    // claim moved to in_repair with rma number
    assert_eq!(s.get_claim(&claim.id).unwrap().status, ClaimStatus::InRepair);
    // closing the RMA closes the claim
    s.update_rma_status(&rma.id, RmaStatus::Closed, "ops").unwrap();
    assert_eq!(s.get_claim(&claim.id).unwrap().status, ClaimStatus::Closed);
}

#[test]
fn reject_sets_reason() {
    let s = store();
    let reg = reg_by_serial(&s, "SN-LAP-0001");
    let claim = s.submit_claim(&reg, "accidental", "screen", 350.0, "alice").unwrap();
    let r = s.reject_claim(&claim.id, "no proof of purchase", "adjuster").unwrap();
    assert_eq!(r.status, ClaimStatus::Rejected);
    assert_eq!(r.denial_reason.as_deref(), Some("no proof of purchase"));
    // can't approve a rejected claim
    assert!(s.approve_claim(&claim.id, "adjuster").is_err());
}

#[test]
fn transfer_respects_plan_transferability() {
    let s = store();
    // standard plan (drill) is NOT transferable; extended (laptop) IS
    let drill = reg_by_serial(&s, "SN-DRL-0001");
    assert!(s.transfer_registration(&drill, "new-owner", "a").is_err());
    let laptop = reg_by_serial(&s, "SN-LAP-0001");
    let t = s.transfer_registration(&laptop, "new-owner", "a").unwrap();
    assert_eq!(t.owner, "new-owner");
}

#[test]
fn analytics_reports_approval_rate() {
    let s = store();
    let reg = reg_by_serial(&s, "SN-LAP-0001");
    let c1 = s.submit_claim(&reg, "accidental", "a", 350.0, "alice").unwrap();
    s.approve_claim(&c1.id, "adj").unwrap();
    let c2 = s.submit_claim(&reg, "accidental", "b", 100.0, "alice").unwrap();
    s.reject_claim(&c2.id, "duplicate", "adj").unwrap();
    let a = s.claims_analytics(None);
    assert!(a["total_claims"].as_u64().unwrap() >= 2);
    assert!(a["total_approved_payout"].as_f64().unwrap() >= 300.0);
    // approval rate is a percentage
    assert!(a["approval_rate_pct"].as_f64().unwrap() > 0.0);
    let _ = Duration::days(1); // silence unused import if any
}
