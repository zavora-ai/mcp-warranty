//! In-memory warranty store with seeded data and engines.
//!
//! Thread-safe via per-collection `Mutex`. IDs come from a monotonic sequence
//! (`PREFIX-{n}` from 1000). Every state change appends to an audit trail.
//! Engines: entitlement/coverage check, claim lifecycle with coverage validation
//! (window, covered failure, exclusions, cap), RMA, and analytics.

use crate::types::*;
use chrono::{Datelike, Duration, NaiveDate, Utc};
use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

pub struct WarrantyStore {
    products: Mutex<HashMap<String, Product>>,
    plans: Mutex<HashMap<String, WarrantyPlan>>,
    registrations: Mutex<HashMap<String, Registration>>,
    claims: Mutex<HashMap<String, Claim>>,
    rmas: Mutex<HashMap<String, Rma>>,
    audit_log: Mutex<Vec<AuditEntry>>,
    seq: Mutex<u64>,
}

impl Default for WarrantyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WarrantyStore {
    pub fn new() -> Self {
        let s = WarrantyStore {
            products: Mutex::new(HashMap::new()),
            plans: Mutex::new(HashMap::new()),
            registrations: Mutex::new(HashMap::new()),
            claims: Mutex::new(HashMap::new()),
            rmas: Mutex::new(HashMap::new()),
            audit_log: Mutex::new(Vec::new()),
            seq: Mutex::new(1000),
        };
        s.seed();
        s
    }

    fn next(&self, prefix: &str) -> String {
        let mut n = self.seq.lock().unwrap();
        *n += 1;
        format!("{prefix}-{n}")
    }

    fn audit(&self, actor: &str, action: &str, detail: impl Into<String>) {
        self.audit_log.lock().unwrap().push(AuditEntry { at: Utc::now(), actor: actor.to_string(), action: action.to_string(), detail: detail.into() });
    }

    // ─── products ──────────────────────────────────────────────────────────

    pub fn create_product(&self, sku: &str, name: &str, category: &str, actor: &str) -> Product {
        let p = Product { id: self.next("PRD"), sku: sku.to_string(), name: name.to_string(), category: category.to_string(), created_at: Utc::now() };
        self.products.lock().unwrap().insert(p.id.clone(), p.clone());
        self.audit(actor, "create_product", p.sku.clone());
        p
    }

    pub fn get_product(&self, id: &str) -> Option<Product> {
        self.products.lock().unwrap().get(id).cloned()
    }

    pub fn list_products(&self, category: Option<&str>) -> Vec<Product> {
        let mut v: Vec<Product> = self.products.lock().unwrap().values().filter(|p| category.is_none_or(|c| p.category.eq_ignore_ascii_case(c))).cloned().collect();
        v.sort_by(|a, b| a.sku.cmp(&b.sku));
        v
    }

    // ─── plans ───────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    pub fn create_plan(&self, name: &str, kind: PlanKind, duration_months: u32, covered: Vec<String>, exclusions: Vec<String>, deductible: f64, coverage_cap: f64, transferable: bool, actor: &str) -> WarrantyPlan {
        let p = WarrantyPlan {
            id: self.next("PLN"),
            name: name.to_string(),
            kind,
            duration_months,
            covered_failures: covered.into_iter().map(|s| s.to_lowercase()).collect(),
            exclusions: exclusions.into_iter().map(|s| s.to_lowercase()).collect(),
            deductible,
            coverage_cap,
            transferable,
            created_at: Utc::now(),
        };
        self.plans.lock().unwrap().insert(p.id.clone(), p.clone());
        self.audit(actor, "create_plan", p.id.clone());
        p
    }

    pub fn get_plan(&self, id: &str) -> Option<WarrantyPlan> {
        self.plans.lock().unwrap().get(id).cloned()
    }

    pub fn list_plans(&self) -> Vec<WarrantyPlan> {
        let mut v: Vec<WarrantyPlan> = self.plans.lock().unwrap().values().cloned().collect();
        v.sort_by(|a, b| a.name.cmp(&b.name));
        v
    }

    // ─── registrations ───────────────────────────────────────────────────

    /// Register a unit under a plan; computes the coverage window from the plan
    /// duration and the purchase date.
    pub fn register(&self, product_id: &str, plan_id: &str, serial_number: &str, owner: &str, purchase_date: NaiveDate, actor: &str) -> Result<Registration, String> {
        if self.get_product(product_id).is_none() { return Err(format!("Product not found: {product_id}")); }
        let plan = self.get_plan(plan_id).ok_or_else(|| format!("Plan not found: {plan_id}"))?;
        let coverage_end = add_months(purchase_date, plan.duration_months);
        let r = Registration {
            id: self.next("REG"),
            product_id: product_id.to_string(),
            plan_id: plan_id.to_string(),
            serial_number: serial_number.to_string(),
            owner: owner.to_string(),
            purchase_date,
            coverage_start: purchase_date,
            coverage_end,
            status: RegistrationStatus::Active,
            claimed_to_date: 0.0,
            created_at: Utc::now(),
        };
        self.registrations.lock().unwrap().insert(r.id.clone(), r.clone());
        self.audit(actor, "register", format!("{} {serial_number}", r.id));
        Ok(r)
    }

    pub fn get_registration(&self, id: &str) -> Option<Registration> {
        self.registrations.lock().unwrap().get(id).cloned()
    }

    /// Resolve a registration by id or serial number.
    pub fn resolve_registration(&self, id_or_serial: &str) -> Option<Registration> {
        let regs = self.registrations.lock().unwrap();
        regs.get(id_or_serial).cloned().or_else(|| regs.values().find(|r| r.serial_number.eq_ignore_ascii_case(id_or_serial)).cloned())
    }

    pub fn list_registrations(&self, owner: Option<&str>, status: Option<RegistrationStatus>) -> Vec<Registration> {
        let mut v: Vec<Registration> = self.registrations.lock().unwrap().values()
            .filter(|r| owner.is_none_or(|o| r.owner.eq_ignore_ascii_case(o)))
            .filter(|r| status.is_none_or(|s| r.status == s))
            .cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        v
    }

    /// Transfer a registration to a new owner (only if the plan is transferable).
    pub fn transfer_registration(&self, registration_id: &str, new_owner: &str, actor: &str) -> Result<Registration, String> {
        let plan_id = {
            let regs = self.registrations.lock().unwrap();
            regs.get(registration_id).map(|r| r.plan_id.clone()).ok_or_else(|| format!("Registration not found: {registration_id}"))?
        };
        let plan = self.get_plan(&plan_id).ok_or("plan missing")?;
        if !plan.transferable { return Err(format!("plan {} is not transferable", plan.name)); }
        let mut regs = self.registrations.lock().unwrap();
        let r = regs.get_mut(registration_id).unwrap();
        r.owner = new_owner.to_string();
        let out = r.clone();
        drop(regs);
        self.audit(actor, "transfer_registration", format!("{registration_id} -> {new_owner}"));
        Ok(out)
    }

    // ─── entitlement / coverage ──────────────────────────────────────────

    /// Check coverage/entitlement for a registration as of a date and (optionally)
    /// a failure type. Returns the active/expired status, days remaining, whether
    /// the failure type is covered, and remaining cap.
    pub fn check_entitlement(&self, registration_id: &str, as_of: NaiveDate, failure_type: Option<&str>) -> Option<serde_json::Value> {
        let reg = self.get_registration(registration_id)?;
        let plan = self.get_plan(&reg.plan_id)?;
        let in_window = as_of >= reg.coverage_start && as_of <= reg.coverage_end && reg.status == RegistrationStatus::Active;
        let days_remaining = (reg.coverage_end - as_of).num_days();
        let (failure_covered, exclusion_hit) = match failure_type {
            Some(ft) => {
                let ft = ft.to_lowercase();
                let excluded = plan.exclusions.iter().any(|e| e == &ft);
                let covered = !excluded && plan.covered_failures.iter().any(|c| c == &ft);
                (Some(covered), excluded)
            }
            None => (None, false),
        };
        let remaining_cap = if plan.coverage_cap > 0.0 { (plan.coverage_cap - reg.claimed_to_date).max(0.0) } else { f64::INFINITY };
        let entitled = in_window && failure_covered.unwrap_or(true);
        Some(serde_json::json!({
            "registration_id": reg.id,
            "serial_number": reg.serial_number,
            "plan": plan.name,
            "status": reg.status,
            "in_coverage_window": in_window,
            "coverage_start": reg.coverage_start,
            "coverage_end": reg.coverage_end,
            "days_remaining": days_remaining,
            "failure_type": failure_type,
            "failure_covered": failure_covered,
            "exclusion_hit": exclusion_hit,
            "deductible": plan.deductible,
            "remaining_cap": if remaining_cap.is_finite() { serde_json::json!(remaining_cap) } else { serde_json::json!("unlimited") },
            "entitled": entitled,
        }))
    }

    // ─── claims lifecycle ────────────────────────────────────────────────

    /// File a claim. Records it as Submitted; coverage is validated at approval.
    pub fn submit_claim(&self, registration_id: &str, failure_type: &str, description: &str, claim_amount: f64, filed_by: &str) -> Result<Claim, String> {
        if self.get_registration(registration_id).is_none() { return Err(format!("Registration not found: {registration_id}")); }
        let now = Utc::now();
        let c = Claim {
            id: self.next("CLM"),
            registration_id: registration_id.to_string(),
            failure_type: failure_type.to_lowercase(),
            description: description.to_string(),
            status: ClaimStatus::Submitted,
            claim_amount,
            approved_amount: None,
            denial_reason: None,
            rma_number: None,
            filed_by: filed_by.to_string(),
            adjudicated_by: None,
            created_at: now,
            updated_at: now,
        };
        self.claims.lock().unwrap().insert(c.id.clone(), c.clone());
        self.audit(filed_by, "submit_claim", format!("{} {}", c.id, failure_type));
        Ok(c)
    }

    pub fn get_claim(&self, id: &str) -> Option<Claim> {
        self.claims.lock().unwrap().get(id).cloned()
    }

    pub fn list_claims(&self, registration_id: Option<&str>, status: Option<ClaimStatus>) -> Vec<Claim> {
        let mut v: Vec<Claim> = self.claims.lock().unwrap().values()
            .filter(|c| registration_id.is_none_or(|r| c.registration_id == r))
            .filter(|c| status.is_none_or(|s| c.status == s))
            .cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        v
    }

    pub fn triage_claim(&self, claim_id: &str, actor: &str) -> Result<Claim, String> {
        let mut claims = self.claims.lock().unwrap();
        let c = claims.get_mut(claim_id).ok_or_else(|| format!("Claim not found: {claim_id}"))?;
        if c.status != ClaimStatus::Submitted { return Err(format!("claim {claim_id} is {:?}, not submitted", c.status)); }
        c.status = ClaimStatus::Triaged;
        c.updated_at = Utc::now();
        let out = c.clone();
        drop(claims);
        self.audit(actor, "triage_claim", claim_id.to_string());
        Ok(out)
    }

    /// Validate coverage for a claim WITHOUT changing it — the adjudication
    /// pre-check (window, failure covered, exclusions, cap). Returns pass + the
    /// payable amount (claim minus deductible, capped at remaining cap).
    pub fn validate_claim(&self, claim_id: &str) -> Result<serde_json::Value, String> {
        let claim = self.get_claim(claim_id).ok_or_else(|| format!("Claim not found: {claim_id}"))?;
        let reg = self.get_registration(&claim.registration_id).ok_or("registration missing")?;
        let plan = self.get_plan(&reg.plan_id).ok_or("plan missing")?;
        let today = Utc::now().date_naive();
        let mut reasons: Vec<String> = Vec::new();
        let in_window = today >= reg.coverage_start && today <= reg.coverage_end;
        if reg.status != RegistrationStatus::Active { reasons.push(format!("registration is {:?}", reg.status)); }
        if !in_window { reasons.push(format!("outside coverage window ({} .. {})", reg.coverage_start, reg.coverage_end)); }
        if plan.exclusions.iter().any(|e| e == &claim.failure_type) { reasons.push(format!("'{}' is excluded", claim.failure_type)); }
        else if !plan.covered_failures.iter().any(|c| c == &claim.failure_type) { reasons.push(format!("'{}' is not a covered failure", claim.failure_type)); }
        let remaining_cap = if plan.coverage_cap > 0.0 { (plan.coverage_cap - reg.claimed_to_date).max(0.0) } else { f64::INFINITY };
        if remaining_cap <= 0.0 { reasons.push("coverage cap exhausted".into()); }
        let net = (claim.claim_amount - plan.deductible).max(0.0);
        let payable = if remaining_cap.is_finite() { net.min(remaining_cap) } else { net };
        Ok(serde_json::json!({
            "claim_id": claim_id,
            "covered": reasons.is_empty(),
            "reasons": reasons,
            "claim_amount": claim.claim_amount,
            "deductible": plan.deductible,
            "remaining_cap": if remaining_cap.is_finite() { serde_json::json!(remaining_cap) } else { serde_json::json!("unlimited") },
            "payable_amount": (payable*100.0).round()/100.0,
        }))
    }

    /// Approve a claim — gated. Re-runs coverage validation and refuses if not
    /// covered. Sets approved (payable) amount and accrues against the plan cap.
    pub fn approve_claim(&self, claim_id: &str, actor: &str) -> Result<Claim, String> {
        let validation = self.validate_claim(claim_id)?;
        if !validation["covered"].as_bool().unwrap_or(false) {
            let reasons = validation["reasons"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("; ")).unwrap_or_default();
            return Err(format!("claim {claim_id} is not covered: {reasons}"));
        }
        let payable = validation["payable_amount"].as_f64().unwrap_or(0.0);
        let mut claims = self.claims.lock().unwrap();
        let c = claims.get_mut(claim_id).ok_or_else(|| format!("Claim not found: {claim_id}"))?;
        if matches!(c.status, ClaimStatus::Approved | ClaimStatus::Closed | ClaimStatus::Rejected) {
            return Err(format!("claim {claim_id} is already {:?}", c.status));
        }
        c.status = ClaimStatus::Approved;
        c.approved_amount = Some(payable);
        c.adjudicated_by = Some(actor.to_string());
        c.updated_at = Utc::now();
        let reg_id = c.registration_id.clone();
        let out = c.clone();
        drop(claims);
        // Accrue against the cap.
        if let Some(r) = self.registrations.lock().unwrap().get_mut(&reg_id) {
            r.claimed_to_date += payable;
        }
        self.audit(actor, "approve_claim", format!("{claim_id} payable {payable}"));
        Ok(out)
    }

    /// Reject a claim with a reason — gated.
    pub fn reject_claim(&self, claim_id: &str, reason: &str, actor: &str) -> Result<Claim, String> {
        let mut claims = self.claims.lock().unwrap();
        let c = claims.get_mut(claim_id).ok_or_else(|| format!("Claim not found: {claim_id}"))?;
        if matches!(c.status, ClaimStatus::Approved | ClaimStatus::Closed | ClaimStatus::Rejected) {
            return Err(format!("claim {claim_id} is already {:?}", c.status));
        }
        c.status = ClaimStatus::Rejected;
        c.denial_reason = Some(reason.to_string());
        c.adjudicated_by = Some(actor.to_string());
        c.updated_at = Utc::now();
        let out = c.clone();
        drop(claims);
        self.audit(actor, "reject_claim", format!("{claim_id}: {reason}"));
        Ok(out)
    }

    pub fn close_claim(&self, claim_id: &str, actor: &str) -> Result<Claim, String> {
        let mut claims = self.claims.lock().unwrap();
        let c = claims.get_mut(claim_id).ok_or_else(|| format!("Claim not found: {claim_id}"))?;
        c.status = ClaimStatus::Closed;
        c.updated_at = Utc::now();
        let out = c.clone();
        drop(claims);
        self.audit(actor, "close_claim", claim_id.to_string());
        Ok(out)
    }

    // ─── RMA ───────────────────────────────────────────────────────────────

    /// Issue an RMA for an approved claim — gated. Moves the claim to InRepair.
    pub fn issue_rma(&self, claim_id: &str, disposition: &str, notes: &str, actor: &str) -> Result<Rma, String> {
        let mut claims = self.claims.lock().unwrap();
        let c = claims.get_mut(claim_id).ok_or_else(|| format!("Claim not found: {claim_id}"))?;
        if c.status != ClaimStatus::Approved {
            return Err(format!("claim {claim_id} must be approved to issue an RMA (is {:?})", c.status));
        }
        let rma_number = format!("RMA{}", &Uuid::new_v4().simple().to_string()[..8].to_uppercase());
        let now = Utc::now();
        let rma = Rma {
            id: self.next("RMA"),
            rma_number: rma_number.clone(),
            claim_id: claim_id.to_string(),
            status: RmaStatus::Issued,
            disposition: disposition.to_string(),
            notes: notes.to_string(),
            issued_by: actor.to_string(),
            created_at: now,
            updated_at: now,
        };
        c.status = ClaimStatus::InRepair;
        c.rma_number = Some(rma_number.clone());
        c.updated_at = now;
        drop(claims);
        self.rmas.lock().unwrap().insert(rma.id.clone(), rma.clone());
        self.audit(actor, "issue_rma", format!("{} for {claim_id}", rma.rma_number));
        Ok(rma)
    }

    pub fn update_rma_status(&self, rma_id: &str, status: RmaStatus, actor: &str) -> Result<Rma, String> {
        let mut rmas = self.rmas.lock().unwrap();
        let rma = rmas.get_mut(rma_id).ok_or_else(|| format!("RMA not found: {rma_id}"))?;
        rma.status = status;
        rma.updated_at = Utc::now();
        let (claim_id, closed) = (rma.claim_id.clone(), status == RmaStatus::Closed);
        let out = rma.clone();
        drop(rmas);
        // Closing the RMA closes the claim.
        if closed {
            if let Some(c) = self.claims.lock().unwrap().get_mut(&claim_id) { c.status = ClaimStatus::Closed; c.updated_at = Utc::now(); }
        }
        self.audit(actor, "update_rma_status", format!("{rma_id} -> {status:?}"));
        Ok(out)
    }

    pub fn get_rma(&self, id: &str) -> Option<Rma> {
        self.rmas.lock().unwrap().get(id).cloned()
    }

    // ─── analytics ─────────────────────────────────────────────────────────

    /// Claims analytics: counts by status, approval rate, total approved payout,
    /// and average payout. Optionally scoped to a product.
    pub fn claims_analytics(&self, product_id: Option<&str>) -> serde_json::Value {
        let regs = self.registrations.lock().unwrap();
        let reg_in_scope: std::collections::HashSet<String> = regs.values()
            .filter(|r| product_id.is_none_or(|p| r.product_id == p))
            .map(|r| r.id.clone()).collect();
        drop(regs);
        let claims = self.claims.lock().unwrap();
        let rel: Vec<&Claim> = claims.values().filter(|c| product_id.is_none() || reg_in_scope.contains(&c.registration_id)).collect();
        let total = rel.len();
        let mut by_status: HashMap<String, u64> = HashMap::new();
        for c in &rel { *by_status.entry(format!("{:?}", c.status).to_lowercase()).or_insert(0) += 1; }
        let adjudicated = rel.iter().filter(|c| matches!(c.status, ClaimStatus::Approved | ClaimStatus::Rejected | ClaimStatus::InRepair | ClaimStatus::Closed)).count();
        let approved = rel.iter().filter(|c| c.approved_amount.is_some()).count();
        let total_payout: f64 = rel.iter().filter_map(|c| c.approved_amount).sum();
        let approval_rate = if adjudicated > 0 { (approved as f64 / adjudicated as f64 * 1000.0).round()/10.0 } else { 0.0 };
        serde_json::json!({
            "product_id": product_id,
            "total_claims": total,
            "by_status": by_status,
            "approval_rate_pct": approval_rate,
            "total_approved_payout": (total_payout*100.0).round()/100.0,
            "avg_payout": if approved > 0 { (total_payout/approved as f64*100.0).round()/100.0 } else { 0.0 },
        })
    }

    pub fn audit_log(&self, limit: usize) -> Vec<AuditEntry> {
        let log = self.audit_log.lock().unwrap();
        log.iter().rev().take(limit).cloned().collect()
    }

    // ─── seed ────────────────────────────────────────────────────────────

    fn seed(&self) {
        let today = Utc::now().date_naive();

        // Products.
        let laptop = self.create_product("LAP-15", "UltraBook 15", "electronics", "system");
        let drill = self.create_product("DRL-PRO", "Pro Cordless Drill", "tools", "system");

        // Plans.
        let std = self.create_plan("1-Year Standard", PlanKind::Standard, 12, vec!["parts".into(), "labor".into()], vec!["cosmetic".into(), "consumables".into()], 0.0, 0.0, false, "system");
        let ext = self.create_plan("3-Year Extended + Accidental", PlanKind::Extended, 36, vec!["parts".into(), "labor".into(), "accidental".into()], vec!["consumables".into()], 50.0, 2000.0, true, "system");

        // Registrations: one in-warranty laptop (extended), one drill on standard near expiry.
        let r1 = self.register(&laptop.id, &ext.id, "SN-LAP-0001", "alice", today - Duration::days(200), "system").unwrap();
        let _r2 = self.register(&drill.id, &std.id, "SN-DRL-0001", "bob", today - Duration::days(360), "system").unwrap();
        // An expired laptop registration (purchased 4y ago on the 3y plan).
        let r3 = self.register(&laptop.id, &ext.id, "SN-LAP-9999", "carol", today - Duration::days(1500), "system").unwrap();
        if let Some(r) = self.registrations.lock().unwrap().get_mut(&r3.id) { r.status = RegistrationStatus::Expired; }

        // A submitted claim on the covered laptop (accidental damage).
        self.submit_claim(&r1.id, "accidental", "Cracked screen from drop", 350.0, "alice").ok();
    }
}

// ─── date helper ───────────────────────────────────────────────────────────

/// Add `months` to a date, clamping the day to the last valid day of the month.
fn add_months(date: NaiveDate, months: u32) -> NaiveDate {
    let total = (date.year() as i32) * 12 + (date.month0() as i32) + months as i32;
    let year = total.div_euclid(12);
    let month0 = total.rem_euclid(12) as u32;
    let month = month0 + 1;
    // Clamp day to month length.
    let last_day = last_day_of_month(year, month);
    let day = date.day().min(last_day);
    NaiveDate::from_ymd_opt(year, month, day).unwrap_or(date)
}

fn last_day_of_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    let first_next = NaiveDate::from_ymd_opt(ny, nm, 1).unwrap();
    (first_next - Duration::days(1)).day()
}
