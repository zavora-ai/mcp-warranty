# Warranty MCP Server

[![Crates.io](https://img.shields.io/crates/v/mcp-warranty.svg)](https://crates.io/crates/mcp-warranty)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![ADK-Rust Enterprise](https://img.shields.io/badge/ADK--Rust-Enterprise-purple.svg)](https://enterprise.adk-rust.com)
[![Registry Ready](https://img.shields.io/badge/ADK_Registry-Ready-green.svg)](https://www.zavora.ai)

A warranty management platform for [ADK-Rust Enterprise](https://enterprise.adk-rust.com) agents. 24 MCP tools covering products, **warranty plans with coverage terms**, registrations with **computed coverage periods**, **entitlement/coverage checks**, a **claims lifecycle with coverage validation**, RMA/repair, and claims/coverage analytics — over a full audit trail.

## A platform, not a point solution

This is modeled as a general warranty/claims backbone (the layer behind a manufacturer or retailer warranty program), so warranty-domain agents — intake, adjudication, and analytics — are clients of one shared system. Coverage rules (window, covered failures, exclusions, deductible, cap, transferability) live in **plans**, so the same engine serves standard manufacturer warranties, paid extended plans, and service contracts.

## Architecture

<p align="center">
  <img src="https://raw.githubusercontent.com/zavora-ai/mcp-warranty/main/docs/architecture.svg" alt="Warranty MCP Architecture" width="780"/>
</p>

## Capabilities

- **Products & plans** — a product catalog and warranty plans carrying coverage terms: duration, covered failure types, exclusions, per-claim deductible, total coverage cap, and transferability.
- **Registrations** — bind a plan to a serial number + owner; the **coverage window is computed** from the plan duration and purchase date (month-accurate, day-clamped). Transfer respects the plan's transferability.
- **Entitlement** — `check_entitlement` reports in-window status, days remaining, whether a failure type is covered or excluded, the deductible, and remaining cap.
- **Claims lifecycle** — submit → triage → approve/reject → in_repair → closed. `validate_claim` is a read-only adjudication pre-check; **`approve_claim` re-runs coverage validation and refuses anything not covered**, sets the payable amount (claim − deductible, capped at remaining cap), and accrues against the plan cap.
- **RMA / repair** — issue an RMA only for an approved claim (repair/replace/refund); status flow issued → received → repaired → shipped → closed; closing the RMA closes the claim.
- **Analytics** — claims by status, approval rate, total and average approved payout, optionally scoped to a product.

## Governance posture

- **Three writes carry financial/contractual weight and are gated** (`requires_approval`, `external_write`): `approve_claim` (commits a payout), `reject_claim` (a contractual denial), and `issue_rma` (commits a repair/replace/refund remedy).
- **Coverage can't be bypassed** — approval re-validates the coverage window, covered-failure list, exclusions, and cap; an expired registration or excluded failure is refused (verified in tests and live).
- **`validate_claim` is read-only** so agents can pre-check adjudication without side effects. Everything material is on the audit trail (`audit_log`).
- Sample data is fictitious.

## Tools (24)

### Products & Plans (6)
`create_product` · `get_product` · `list_products` · `create_plan` · `get_plan` · `list_plans`

### Registrations & Entitlement (5)
`register` · `get_registration` · `list_registrations` · `transfer_registration` · `check_entitlement`

### Claims (8)
`submit_claim` · `get_claim` · `list_claims` · `triage_claim` · `validate_claim` · `approve_claim` (gated) · `reject_claim` (gated) · `close_claim`

### RMA & Analytics (5)
`issue_rma` (gated) · `update_rma_status` · `get_rma` · `claims_analytics` · `audit_log`

## Example

```jsonc
// Register, check entitlement, file + validate a claim
{"name": "register", "arguments": {"product_id": "PRD-1000", "plan_id": "PLN-1003",
  "serial_number": "SN-123", "owner": "alice", "purchase_date": "2025-01-10"}}
{"name": "check_entitlement", "arguments": {"registration_id": "REG-1005", "failure_type": "accidental"}}
{"name": "submit_claim", "arguments": {"registration_id": "REG-1005", "failure_type": "accidental",
  "description": "cracked screen", "claim_amount": 350}}
{"name": "validate_claim", "arguments": {"claim_id": "CLM-1009"}}

// Gated adjudication + remedy
{"name": "approve_claim", "arguments": {"claim_id": "CLM-1009"}}
{"name": "issue_rma", "arguments": {"claim_id": "CLM-1009", "disposition": "repair"}}
```

## Install & run

```bash
cargo install mcp-warranty
mcp-warranty            # serves MCP over stdio
```

Or build from source:

```bash
git clone https://github.com/zavora-ai/mcp-warranty
cd mcp-warranty && cargo build --release
./target/release/mcp-warranty
```

## Registry manifest

```toml
server_id = "mcp_warranty"
display_name = "Warranty Management"
version = "1.0.0"
domain = "warranty"
risk_level = "high"
writes_allowed = "gated"
```

The full [`mcp-server.toml`](mcp-server.toml) declares all 24 tools with risk classes and approval gates for registry onboarding.

## License

Apache-2.0

## rmcp and MCP compatibility

This server is built with [`rmcp` 3.1.2](https://github.com/modelcontextprotocol/rust-sdk/releases/tag/rmcp-v3.1.2) and requires Rust 1.88 or newer. The rmcp 3 rollout retains legacy MCP initialization compatibility and targets MCP protocol revisions `2025-11-25` and `2026-07-28`.
