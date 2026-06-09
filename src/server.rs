//! MCP tool surface for the warranty platform.
//!
//! Reads (catalog, registrations, entitlement, validate, claims, analytics) are
//! `read_only`. Most writes are `internal_write`. Three carry financial /
//! contractual weight and are gated (`requires_approval`): `approve_claim` and
//! `reject_claim` (adjudication — `external_write`) and `issue_rma` (commits a
//! repair/replace/refund — `external_write`).

use crate::store::WarrantyStore;
use crate::types::*;
use adk_mcp_sdk::{HealthCheck, HealthStatus};
use chrono::NaiveDate;
use rmcp::{handler::server::wrapper::Parameters, schemars, tool, tool_router};
use serde::Deserialize;
use std::sync::Arc;

fn dactor() -> String { "agent".into() }
fn date(s: &Option<String>) -> Option<NaiveDate> { s.as_ref().and_then(|x| NaiveDate::parse_from_str(x, "%Y-%m-%d").ok()) }
fn today() -> NaiveDate { chrono::Utc::now().date_naive() }
fn dlimit() -> usize { 50 }

// ─── inputs ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreateProductInput { pub sku: String, #[serde(default)] pub name: String, #[serde(default)] pub category: String, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ProductIdInput { pub product_id: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListProductsInput { pub category: Option<String> }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EmptyInput {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CreatePlanInput {
    pub name: String,
    #[serde(default = "dstd")] pub kind: PlanKind,
    #[serde(default = "dtwelve")] pub duration_months: u32,
    #[serde(default)] pub covered_failures: Vec<String>,
    #[serde(default)] pub exclusions: Vec<String>,
    #[serde(default)] pub deductible: f64,
    #[serde(default)] pub coverage_cap: f64,
    #[serde(default)] pub transferable: bool,
    #[serde(default = "dactor")] pub actor: String,
}
fn dstd() -> PlanKind { PlanKind::Standard }
fn dtwelve() -> u32 { 12 }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct PlanIdInput { pub plan_id: String }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RegisterInput { pub product_id: String, pub plan_id: String, pub serial_number: String, pub owner: String, pub purchase_date: Option<String>, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ResolveRegInput { pub registration: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListRegsInput { pub owner: Option<String>, pub status: Option<RegistrationStatus> }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct TransferInput { pub registration_id: String, pub new_owner: String, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EntitlementInput { pub registration_id: String, pub as_of: Option<String>, pub failure_type: Option<String> }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SubmitClaimInput { pub registration_id: String, pub failure_type: String, #[serde(default)] pub description: String, #[serde(default)] pub claim_amount: f64, #[serde(default = "dactor")] pub filed_by: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ClaimIdInput { pub claim_id: String, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ClaimIdOnlyInput { pub claim_id: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ListClaimsInput { pub registration_id: Option<String>, pub status: Option<ClaimStatus> }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RejectClaimInput { pub claim_id: String, pub reason: String, #[serde(default = "dactor")] pub actor: String }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct IssueRmaInput { pub claim_id: String, #[serde(default = "drepair")] pub disposition: String, #[serde(default)] pub notes: String, #[serde(default = "dactor")] pub actor: String }
fn drepair() -> String { "repair".into() }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct UpdateRmaInput { pub rma_id: String, pub status: RmaStatus, #[serde(default = "dactor")] pub actor: String }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct RmaIdInput { pub rma_id: String }

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyticsInput { pub product_id: Option<String> }
#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AuditLogInput { #[serde(default = "dlimit")] pub limit: usize }

// ─── server ───────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct WarrantyServer { pub store: Arc<WarrantyStore> }

#[tool_router(server_handler)]
impl WarrantyServer {
    // products
    #[tool(description = "Create a product in the catalog.")]
    fn create_product(&self, Parameters(i): Parameters<CreateProductInput>) -> String {
        let p = self.store.create_product(&i.sku, &i.name, &i.category, &i.actor);
        serde_json::to_string_pretty(&p).unwrap()
    }

    #[tool(description = "Get a product by id.")]
    fn get_product(&self, Parameters(i): Parameters<ProductIdInput>) -> String {
        match self.store.get_product(&i.product_id) {
            Some(p) => serde_json::to_string_pretty(&p).unwrap(), None => format!("Product not found: {}", i.product_id) }
    }

    #[tool(description = "List products, optionally by category.")]
    fn list_products(&self, Parameters(i): Parameters<ListProductsInput>) -> String {
        let v = self.store.list_products(i.category.as_deref());
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "products": v})).unwrap()
    }

    // plans
    #[tool(description = "Create a warranty plan: kind (standard/extended/service_contract), duration months, covered_failures, exclusions, deductible, coverage_cap (0 = unlimited), transferable.")]
    fn create_plan(&self, Parameters(i): Parameters<CreatePlanInput>) -> String {
        let p = self.store.create_plan(&i.name, i.kind, i.duration_months, i.covered_failures, i.exclusions, i.deductible, i.coverage_cap, i.transferable, &i.actor);
        serde_json::to_string_pretty(&p).unwrap()
    }

    #[tool(description = "Get a warranty plan and its coverage terms.")]
    fn get_plan(&self, Parameters(i): Parameters<PlanIdInput>) -> String {
        match self.store.get_plan(&i.plan_id) {
            Some(p) => serde_json::to_string_pretty(&p).unwrap(), None => format!("Plan not found: {}", i.plan_id) }
    }

    #[tool(description = "List warranty plans.")]
    fn list_plans(&self, Parameters(_): Parameters<EmptyInput>) -> String {
        let v = self.store.list_plans();
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "plans": v})).unwrap()
    }

    // registrations
    #[tool(description = "Register a unit under a plan (binds serial + owner; computes the coverage window from the plan duration and purchase date).")]
    fn register(&self, Parameters(i): Parameters<RegisterInput>) -> String {
        let pd = date(&i.purchase_date).unwrap_or_else(today);
        match self.store.register(&i.product_id, &i.plan_id, &i.serial_number, &i.owner, pd, &i.actor) {
            Ok(r) => serde_json::to_string_pretty(&r).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Get a registration by id or serial number.")]
    fn get_registration(&self, Parameters(i): Parameters<ResolveRegInput>) -> String {
        match self.store.resolve_registration(&i.registration) {
            Some(r) => serde_json::to_string_pretty(&r).unwrap(), None => format!("Registration not found: {}", i.registration) }
    }

    #[tool(description = "List registrations, optionally by owner and/or status.")]
    fn list_registrations(&self, Parameters(i): Parameters<ListRegsInput>) -> String {
        let v = self.store.list_registrations(i.owner.as_deref(), i.status);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "registrations": v})).unwrap()
    }

    #[tool(description = "Transfer a registration to a new owner (only if the plan is transferable).")]
    fn transfer_registration(&self, Parameters(i): Parameters<TransferInput>) -> String {
        match self.store.transfer_registration(&i.registration_id, &i.new_owner, &i.actor) {
            Ok(r) => serde_json::to_string_pretty(&r).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Check coverage/entitlement for a registration as of a date and optional failure type: in-window, days remaining, covered/excluded, deductible, remaining cap, and an overall entitled flag.")]
    fn check_entitlement(&self, Parameters(i): Parameters<EntitlementInput>) -> String {
        let as_of = date(&i.as_of).unwrap_or_else(today);
        match self.store.check_entitlement(&i.registration_id, as_of, i.failure_type.as_deref()) {
            Some(v) => serde_json::to_string_pretty(&v).unwrap(), None => format!("Registration not found: {}", i.registration_id) }
    }

    // claims
    #[tool(description = "File (submit) a warranty claim against a registration. Coverage is validated at approval.")]
    fn submit_claim(&self, Parameters(i): Parameters<SubmitClaimInput>) -> String {
        match self.store.submit_claim(&i.registration_id, &i.failure_type, &i.description, i.claim_amount, &i.filed_by) {
            Ok(c) => serde_json::to_string_pretty(&c).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Get a claim by id.")]
    fn get_claim(&self, Parameters(i): Parameters<ClaimIdOnlyInput>) -> String {
        match self.store.get_claim(&i.claim_id) {
            Some(c) => serde_json::to_string_pretty(&c).unwrap(), None => format!("Claim not found: {}", i.claim_id) }
    }

    #[tool(description = "List claims, optionally by registration and/or status.")]
    fn list_claims(&self, Parameters(i): Parameters<ListClaimsInput>) -> String {
        let v = self.store.list_claims(i.registration_id.as_deref(), i.status);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "claims": v})).unwrap()
    }

    #[tool(description = "Move a submitted claim to triaged (under review).")]
    fn triage_claim(&self, Parameters(i): Parameters<ClaimIdInput>) -> String {
        match self.store.triage_claim(&i.claim_id, &i.actor) {
            Ok(c) => serde_json::to_string_pretty(&c).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Validate a claim's coverage WITHOUT changing it (adjudication pre-check): window, covered failure, exclusions, cap, and the payable amount (claim minus deductible, capped). Read-only.")]
    fn validate_claim(&self, Parameters(i): Parameters<ClaimIdOnlyInput>) -> String {
        match self.store.validate_claim(&i.claim_id) {
            Ok(v) => serde_json::to_string_pretty(&v).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Approve a claim. Re-runs coverage validation and refuses if not covered; sets the payable amount (net of deductible/cap) and accrues against the plan cap. Financial decision — gated.")]
    fn approve_claim(&self, Parameters(i): Parameters<ClaimIdInput>) -> String {
        match self.store.approve_claim(&i.claim_id, &i.actor) {
            Ok(c) => serde_json::to_string_pretty(&c).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Reject a claim with a reason. Contractual decision — gated.")]
    fn reject_claim(&self, Parameters(i): Parameters<RejectClaimInput>) -> String {
        match self.store.reject_claim(&i.claim_id, &i.reason, &i.actor) {
            Ok(c) => serde_json::to_string_pretty(&c).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Close a claim (terminal).")]
    fn close_claim(&self, Parameters(i): Parameters<ClaimIdInput>) -> String {
        match self.store.close_claim(&i.claim_id, &i.actor) {
            Ok(c) => serde_json::to_string_pretty(&c).unwrap(), Err(e) => format!("Error: {e}") }
    }

    // RMA
    #[tool(description = "Issue an RMA for an APPROVED claim (disposition: repair/replace/refund); moves the claim to in_repair. Commits a remedy — gated.")]
    fn issue_rma(&self, Parameters(i): Parameters<IssueRmaInput>) -> String {
        match self.store.issue_rma(&i.claim_id, &i.disposition, &i.notes, &i.actor) {
            Ok(r) => serde_json::to_string_pretty(&r).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Update an RMA's status (issued/received/repaired/shipped/closed). Closing the RMA closes the claim.")]
    fn update_rma_status(&self, Parameters(i): Parameters<UpdateRmaInput>) -> String {
        match self.store.update_rma_status(&i.rma_id, i.status, &i.actor) {
            Ok(r) => serde_json::to_string_pretty(&r).unwrap(), Err(e) => format!("Error: {e}") }
    }

    #[tool(description = "Get an RMA by id.")]
    fn get_rma(&self, Parameters(i): Parameters<RmaIdInput>) -> String {
        match self.store.get_rma(&i.rma_id) {
            Some(r) => serde_json::to_string_pretty(&r).unwrap(), None => format!("RMA not found: {}", i.rma_id) }
    }

    // analytics
    #[tool(description = "Claims analytics: counts by status, approval rate, total approved payout, and average payout — optionally scoped to a product.")]
    fn claims_analytics(&self, Parameters(i): Parameters<AnalyticsInput>) -> String {
        serde_json::to_string_pretty(&self.store.claims_analytics(i.product_id.as_deref())).unwrap()
    }

    #[tool(description = "Recent audit-trail entries (most recent first).")]
    fn audit_log(&self, Parameters(i): Parameters<AuditLogInput>) -> String {
        let v = self.store.audit_log(i.limit);
        serde_json::to_string_pretty(&serde_json::json!({"count": v.len(), "entries": v})).unwrap()
    }
}

#[async_trait::async_trait]
impl HealthCheck for WarrantyServer {
    async fn check_health(&self) -> HealthStatus {
        HealthStatus { healthy: true, message: Some("operational".into()), latency_ms: Some(1) }
    }
}
