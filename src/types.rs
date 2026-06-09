//! Warranty management platform domain model.
//!
//! Broad warranty platform: products, warranty plans with coverage terms,
//! registrations that bind a plan to a unit and establish a coverage period,
//! entitlement/coverage checks, a claims lifecycle (submit → triage →
//! approve/reject → repair → close) with coverage validation, RMA/repair, and
//! claims/coverage analytics. The warranty-domain agents are clients.

use chrono::{DateTime, NaiveDate, Utc};
use rmcp::schemars;
use serde::{Deserialize, Serialize};

// ─── products ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Product {
    pub id: String,
    pub sku: String,
    pub name: String,
    pub category: String,
    pub created_at: DateTime<Utc>,
}

// ─── warranty plans ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanKind {
    /// Included with purchase.
    Standard,
    /// Paid add-on.
    Extended,
    /// Service contract.
    ServiceContract,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WarrantyPlan {
    pub id: String,
    pub name: String,
    pub kind: PlanKind,
    /// Coverage duration in months from registration/purchase.
    pub duration_months: u32,
    /// What's covered, e.g. ["parts", "labor", "accidental"].
    pub covered_failures: Vec<String>,
    /// Explicit exclusions, e.g. ["cosmetic", "consumables"].
    pub exclusions: Vec<String>,
    /// Per-claim deductible.
    pub deductible: f64,
    /// Max total payout over the plan life (0 = unlimited).
    pub coverage_cap: f64,
    /// Whether the plan is transferable to a new owner.
    pub transferable: bool,
    pub created_at: DateTime<Utc>,
}

// ─── registrations ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationStatus {
    Active,
    Expired,
    Void,
}

/// A registered unit: a product + plan bound to a serial number and owner, with
/// a coverage window.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Registration {
    pub id: String,
    pub product_id: String,
    pub plan_id: String,
    pub serial_number: String,
    pub owner: String,
    pub purchase_date: NaiveDate,
    pub coverage_start: NaiveDate,
    pub coverage_end: NaiveDate,
    pub status: RegistrationStatus,
    /// Cumulative approved payout so far (against the plan cap).
    pub claimed_to_date: f64,
    pub created_at: DateTime<Utc>,
}

// ─── claims ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimStatus {
    Submitted,
    /// Under review.
    Triaged,
    Approved,
    Rejected,
    /// Repair/replacement in progress.
    InRepair,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Claim {
    pub id: String,
    pub registration_id: String,
    /// Failure category claimed, e.g. "parts", "accidental".
    pub failure_type: String,
    pub description: String,
    pub status: ClaimStatus,
    /// Requested/estimated repair cost.
    pub claim_amount: f64,
    /// Approved payout (set on approval, net of deductible / cap).
    pub approved_amount: Option<f64>,
    pub denial_reason: Option<String>,
    pub rma_number: Option<String>,
    pub filed_by: String,
    pub adjudicated_by: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── RMA ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, schemars::JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RmaStatus {
    Issued,
    Received,
    Repaired,
    Shipped,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Rma {
    pub id: String,
    pub rma_number: String,
    pub claim_id: String,
    pub status: RmaStatus,
    /// repair | replace | refund
    pub disposition: String,
    pub notes: String,
    pub issued_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── audit trail ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct AuditEntry {
    pub at: DateTime<Utc>,
    pub actor: String,
    pub action: String,
    pub detail: String,
}
