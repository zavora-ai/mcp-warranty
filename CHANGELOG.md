# Changelog

## [1.1.0] - 2026-08-13

### Changed
- Upgraded to rmcp 3.1.2 and raised the minimum supported Rust version to 1.94.1.
- Added MCP 2026-07-28 stateless request handling while retaining MCP 2025-11-25 initialization compatibility.

### Added
- Per-request identity and protocol metadata, on-demand discovery/cache hints, and the configured Tasks and sealed MRTR approval policies.

## [1.0.0] - 2026-06-10

Initial release — a broad warranty management platform.

### Added
- **Products & plans** — catalog plus warranty plans with coverage terms (duration, covered failures, exclusions, deductible, cap, transferability)
  (`create_product`, `get_product`, `list_products`, `create_plan`, `get_plan`, `list_plans`)
- **Registrations & entitlement** — bind plan to serial/owner with a computed coverage window; transfer (if transferable); coverage/entitlement checks
  (`register`, `get_registration`, `list_registrations`, `transfer_registration`, `check_entitlement`)
- **Claims lifecycle** — submit → triage → approve/reject → in_repair → closed; read-only `validate_claim` pre-check; approval re-validates coverage and applies deductible/cap, accruing against the plan cap
  (`submit_claim`, `get_claim`, `list_claims`, `triage_claim`, `validate_claim`, `approve_claim`, `reject_claim`, `close_claim`)
- **RMA & analytics** — RMA only for approved claims (repair/replace/refund) with a status flow that closes the claim; claims analytics (status mix, approval rate, payouts)
  (`issue_rma`, `update_rma_status`, `get_rma`, `claims_analytics`, `audit_log`)
- 24 tools total; `approve_claim`, `reject_claim`, and `issue_rma` (external writes with financial/contractual weight) are approval-gated; full audit trail.
- 16 tests (12 integration + 4 manifest); verified end-to-end over MCP stdio.
