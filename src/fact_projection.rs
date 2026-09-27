use chrono::{DateTime, Utc};
use univers_aip_contracts_data::core::DataPointResult;
use univers_aip_contracts_world::evidence::{FactQuery, FactStore};
use univers_aip_contracts_world::operating::OperatingFactValue;

/// Project accepted Facts into the operating value shape used by vertical domains.
pub async fn list_fact_values<S: FactStore + ?Sized>(
    store: &S,
    query: FactQuery,
) -> DataPointResult<Vec<OperatingFactValue>> {
    store
        .list_facts(query)
        .await
        .map(|facts| facts.into_iter().map(OperatingFactValue::from).collect())
}

/// Return the latest accepted Fact by `asserted_at`.
///
/// Caller limits do not apply because the policy must see every matching
/// candidate before ranking.
pub async fn latest_fact_value<S: FactStore + ?Sized>(
    store: &S,
    mut query: FactQuery,
) -> DataPointResult<Option<OperatingFactValue>> {
    query.limit = None;
    Ok(list_fact_values(store, query)
        .await?
        .into_iter()
        .max_by_key(|value| value.asserted_at))
}

/// Resolve the latest assertion that is valid at an explicit point in time.
///
/// Validity is half-open (`valid_from <= at < valid_to`). Assertions without
/// bounds remain valid until superseded by a later assertion selected here.
pub async fn fact_value_at<S: FactStore + ?Sized>(
    store: &S,
    mut query: FactQuery,
    at: DateTime<Utc>,
) -> DataPointResult<Option<OperatingFactValue>> {
    query.valid_at = Some(at);
    query.limit = None;
    query.validate()?;
    Ok(list_fact_values(store, query)
        .await?
        .into_iter()
        .max_by_key(|value| value.asserted_at))
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use chrono::{Duration, Utc};
    use univers_aip_contracts_data::core::{DataPointError, DataProvenance, DataWriteResult};
    use univers_aip_contracts_world::evidence::DataFact;

    use super::*;

    struct InMemoryFactStore(Vec<DataFact>);

    #[async_trait]
    impl FactStore for InMemoryFactStore {
        async fn write_fact(
            &self,
            _organization_id: &str,
            _fact: DataFact,
            _idempotency_key: &str,
        ) -> DataPointResult<DataWriteResult<DataFact>> {
            Err(DataPointError::InvalidOperation(
                "read-only test FactStore".to_string(),
            ))
        }

        async fn write_fact_from_evidence(
            &self,
            _organization_id: &str,
            _evidence_id: &str,
            _evidence_pointer: &str,
            _fact: DataFact,
            _idempotency_key: &str,
        ) -> DataPointResult<DataWriteResult<DataFact>> {
            Err(DataPointError::InvalidOperation(
                "read-only test FactStore".to_string(),
            ))
        }

        async fn get_fact(&self, _id: &str) -> DataPointResult<DataFact> {
            Err(DataPointError::InvalidOperation(
                "read-only test FactStore".to_string(),
            ))
        }

        async fn list_facts(&self, query: FactQuery) -> DataPointResult<Vec<DataFact>> {
            Ok(self
                .0
                .iter()
                .filter(|fact| query.matches(fact))
                .take(query.limit.unwrap_or(usize::MAX))
                .cloned()
                .collect())
        }
    }

    #[tokio::test]
    async fn latest_projection_ignores_caller_limit_and_uses_asserted_at() {
        let now = Utc::now();
        let mut older = DataFact::new_at(
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
            "org-test",
            "subject-1",
            "lighting.luminaire_group.installed_wattage_w",
            serde_json::json!(100.0),
            DataProvenance::Nominal,
        )
        .unwrap();
        older.asserted_at = now - Duration::minutes(5);
        let mut newer = DataFact::new_at(
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
            "org-test",
            "subject-1",
            "lighting.luminaire_group.installed_wattage_w",
            serde_json::json!(250.0),
            DataProvenance::Nominal,
        )
        .unwrap();
        newer.asserted_at = now;

        let store = InMemoryFactStore(vec![newer, older]);
        let latest = latest_fact_value(
            &store,
            FactQuery {
                organization_id: Some("org-test".to_string()),
                subject_id: Some("subject-1".to_string()),
                field: Some("lighting.luminaire_group.installed_wattage_w".to_string()),
                limit: Some(1),
                ..FactQuery::default()
            },
        )
        .await
        .expect("latest fact")
        .expect("value");

        assert_eq!(latest.value, serde_json::json!(250.0));
    }

    #[tokio::test]
    async fn temporal_projection_honors_half_open_fact_validity() {
        let now = Utc::now();
        let mut expired = DataFact::new_at(
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
            "org-test",
            "subject-1",
            "environment.building.geometry",
            serde_json::json!({"artifactRef": "artifact://old"}),
            DataProvenance::Reported,
        )
        .unwrap();
        expired.valid_from = Some(now - Duration::hours(2));
        expired.valid_to = Some(now - Duration::hours(1));
        expired.asserted_at = now;

        let mut current = DataFact::new_at(
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
            "org-test",
            "subject-1",
            "environment.building.geometry",
            serde_json::json!({"artifactRef": "artifact://current"}),
            DataProvenance::Reported,
        )
        .unwrap();
        current.valid_from = Some(now - Duration::hours(1));
        current.asserted_at = now - Duration::minutes(5);

        let store = InMemoryFactStore(vec![expired, current]);
        let resolved = fact_value_at(
            &store,
            FactQuery {
                organization_id: Some("org-test".to_string()),
                subject_ids: vec!["subject-1".to_string()],
                fields: vec!["environment.building.geometry".to_string()],
                ..FactQuery::default()
            },
            now,
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(
            resolved.value["artifactRef"],
            serde_json::json!("artifact://current")
        );
    }
}
