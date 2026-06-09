//! Warranty MCP Server library surface.
//!
//! A warranty management platform: products, warranty plans with coverage terms,
//! registrations with coverage periods, entitlement/coverage checks, a claims
//! lifecycle (submit → triage → approve/reject → repair → close) with coverage
//! validation, RMA/repair, and claims/coverage analytics — over an audit trail.

pub mod server;
pub mod store;
pub mod types;
