use chrono::Utc;
use univers_aip_contracts_data::core::{
    DataKind, DataPointError, DataPointResult, DataRef, DataWriteOutcome, DataWriteReceipt,
    DataWriteResult,
};
use univers_aip_contracts_world::evidence::{DataFact, FactQuery, FactStore};

/// Outcome of asserting a Fact value against the latest accepted assertion.
#[derive(Debug, Clone, PartialEq)]
pub enum FactAssertOutcome {
    /// The value changed or the field had no assertion yet.
    Created(DataWriteResult<DataFact>),
    /// The latest assertion already carries this value.
    Unchanged(DataWriteResult<DataFact>),
}

impl FactAssertOutcome {
    pub fn fact(&self) -> &DataFact {
        match self {
            Self::Created(result) | Self::Unchanged(result) => &result.value,
        }
    }

    pub fn receipt(&self) -> &DataWriteReceipt {
        match self {
            Self::Created(result) | Self::Unchanged(result) => &result.receipt,
        }
    }
}

/// Append a Fact assertion when its value changes, or return an observable
/// retry-safe no-op when the latest assertion already carries the same value.
pub async fn assert_fact_for_organization<S: FactStore + ?Sized>(
    store: &S,
    organization_id: &str,
    mut fact: DataFact,
    idempotency_key: &str,
) -> DataPointResult<FactAssertOutcome> {
    if fact.organization_id != organization_id {
        return Err(DataPointError::InvalidInput(format!(
            "Fact organization_id {} does not match assertion scope {organization_id}",
            fact.organization_id
        )));
    }
    let latest = store
        .list_facts(FactQuery {
            organization_id: Some(organization_id.to_string()),
            subject_id: Some(fact.subject_id.clone()),
            field: Some(fact.field.clone()),
            ..FactQuery::default()
        })
        .await?
        .into_iter()
        .max_by_key(|candidate| candidate.asserted_at);
    if let Some(latest) = latest {
        if latest.value == fact.value {
            return Ok(FactAssertOutcome::Unchanged(fact_assertion_write_result(
                organization_id,
                latest,
                DataWriteOutcome::Skipped,
                idempotency_key,
            )?));
        }
    }

    let base = fact
        .fact_key
        .take()
        .unwrap_or_else(|| format!("{}:{}", fact.subject_id, fact.field));
    fact.fact_key = Some(format!(
        "{base}:assertion:{}",
        stable_key_fingerprint(idempotency_key)
    ));
    let result = store
        .write_fact(organization_id, fact, idempotency_key)
        .await?;
    match result.receipt.outcome {
        DataWriteOutcome::Created => Ok(FactAssertOutcome::Created(result)),
        DataWriteOutcome::Skipped => Ok(FactAssertOutcome::Unchanged(result)),
        outcome => Err(DataPointError::InvalidOperation(format!(
            "Fact assertion returned unsupported write outcome {outcome:?}"
        ))),
    }
}

fn stable_key_fingerprint(value: &str) -> String {
    uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_URL, value.as_bytes())
        .simple()
        .to_string()
}

fn fact_assertion_write_result(
    organization_id: &str,
    fact: DataFact,
    outcome: DataWriteOutcome,
    idempotency_key: &str,
) -> DataPointResult<DataWriteResult<DataFact>> {
    let reference = DataRef::new(organization_id, DataKind::Fact, fact.id.clone())?;
    let mut receipt = DataWriteReceipt::new(outcome, reference, Utc::now())
        .with_idempotency_key(idempotency_key)?;
    if outcome == DataWriteOutcome::Skipped {
        receipt.mark_replayed()?;
    }
    Ok(DataWriteResult::new(fact, receipt))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use univers_aip_contracts_data::core::DataProvenance;

    use super::*;

    #[derive(Default)]
    struct MemFacts(Mutex<Vec<DataFact>>);

    #[async_trait::async_trait]
    impl FactStore for MemFacts {
        async fn write_fact(
            &self,
            organization_id: &str,
            mut fact: DataFact,
            idempotency_key: &str,
        ) -> DataPointResult<DataWriteResult<DataFact>> {
            let write_key = fact.fact_key.as_deref().map_or_else(
                || format!("idempotency:{idempotency_key}"),
                |key| format!("fact:{key}"),
            );
            fact.id = DataFact::stable_id_for_write_key(organization_id, &write_key);
            let mut rows = self.0.lock().unwrap();
            if let Some(existing) = rows.iter().find(|existing| existing.id == fact.id) {
                if existing != &fact {
                    return Err(DataPointError::AlreadyExists(format!(
                        "Fact write identity {} already carries a different payload",
                        fact.id
                    )));
                }
                return fact_assertion_write_result(
                    organization_id,
                    existing.clone(),
                    DataWriteOutcome::Skipped,
                    idempotency_key,
                );
            }
            rows.push(fact.clone());
            fact_assertion_write_result(
                organization_id,
                fact,
                DataWriteOutcome::Created,
                idempotency_key,
            )
        }

        async fn write_fact_from_evidence(
            &self,
            organization_id: &str,
            _evidence_id: &str,
            _evidence_pointer: &str,
            fact: DataFact,
            idempotency_key: &str,
        ) -> DataPointResult<DataWriteResult<DataFact>> {
            self.write_fact(organization_id, fact, idempotency_key)
                .await
        }

        async fn get_fact(&self, id: &str) -> DataPointResult<DataFact> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .find(|fact| fact.id == id)
                .cloned()
                .ok_or_else(|| DataPointError::NotFound(id.to_string()))
        }

        async fn list_facts(&self, query: FactQuery) -> DataPointResult<Vec<DataFact>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|fact| {
                    query
                        .organization_id
                        .as_deref()
                        .is_none_or(|organization_id| fact.organization_id == organization_id)
                        && query
                            .subject_id
                            .as_deref()
                            .is_none_or(|subject_id| fact.subject_id == subject_id)
                        && query
                            .field
                            .as_deref()
                            .is_none_or(|field| fact.field == field)
                })
                .cloned()
                .collect())
        }
    }

    fn fact(value: f64) -> DataFact {
        DataFact::new_at(
            uuid::Uuid::new_v4().to_string(),
            chrono::Utc::now(),
            "org-1",
            "environment--zone--z1",
            "environment.zone.area_m2",
            serde_json::json!(value),
            DataProvenance::Reported,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn explicit_assertion_keys_preserve_a_b_a_history_and_retry_safety() {
        let store = MemFacts::default();
        let first = assert_fact_for_organization(&store, "org-1", fact(42.5), "reading-1")
            .await
            .unwrap();
        let changed = assert_fact_for_organization(&store, "org-1", fact(43.0), "reading-2")
            .await
            .unwrap();
        let changed_back = assert_fact_for_organization(&store, "org-1", fact(42.5), "reading-3")
            .await
            .unwrap();
        let replay = assert_fact_for_organization(&store, "org-1", fact(42.5), "reading-3")
            .await
            .unwrap();

        assert!(matches!(first, FactAssertOutcome::Created(_)));
        assert!(matches!(changed, FactAssertOutcome::Created(_)));
        assert!(matches!(changed_back, FactAssertOutcome::Created(_)));
        assert!(matches!(replay, FactAssertOutcome::Unchanged(_)));
        assert_eq!(store.0.lock().unwrap().len(), 3);
    }
}
