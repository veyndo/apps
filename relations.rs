//! Shared relation projections are maintained transactionally by app_record_project; wire values remain bounded ordered arrays.
use super::*;
pub(super) fn valid(value: &Value) -> bool {
    value.as_array().is_some_and(|ids| {
        ids.len() <= 100
            && ids
                .iter()
                .all(|id| id.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 100))
            && ids.iter().collect::<std::collections::HashSet<_>>().len() == ids.len()
    })
}
