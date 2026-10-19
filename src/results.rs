use crate::model::NodeId;
use crate::version::{Store, StoreError, VersionId};

/// Versions of `node` that appear in a view whose recorded tests all passed and
/// that has a result for `bench`, with that median, fastest first, ties by
/// version id.
pub fn passing_by_bench(
    store: &dyn Store,
    node: &NodeId,
    bench: &NodeId,
) -> Result<Vec<(VersionId, u64)>, StoreError> {
    let _ = (store, node, bench);
    todo!()
}

/// Failure messages recorded for tests that reach `node` in views where it
/// used a version that is not the latest, oldest first, for use in prompts.
pub fn failures_for(store: &dyn Store, node: &NodeId) -> Result<Vec<String>, StoreError> {
    let _ = (store, node);
    todo!()
}
