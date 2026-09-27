//! Retry-safe Fact assertion and read projection over caller-provided pure World Ports.
mod fact_assertion;
mod fact_projection;

pub use fact_assertion::{assert_fact_for_organization, FactAssertOutcome};
pub use fact_projection::{fact_value_at, latest_fact_value, list_fact_values};
