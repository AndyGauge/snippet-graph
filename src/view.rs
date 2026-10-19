use crate::model::{Graph, NodeId};
use crate::version::{Origin, RunId, Store, StoreError, VersionId, View, ViewId};

/// Store a version for every node in `graph` (reusing identical content) and
/// return the view over them. `parent` is the view this one derives from; each
/// node's new version records its previous version in that view as its parent.
pub fn from_graph(
    store: &mut dyn Store,
    graph: &Graph,
    run: &RunId,
    origin: &Origin,
    parent: Option<&View>,
) -> Result<View, StoreError> {
    let _ = (store, graph, run, origin, parent);
    todo!()
}

/// Render a view back to a graph, with nodes from stored versions and edges from
/// the view. `to_graph(from_graph(g)) == g`.
pub fn to_graph(store: &dyn Store, view: &View) -> Result<Graph, StoreError> {
    let _ = (store, view);
    todo!()
}

/// Nodes whose version differs between the views, including nodes present in
/// only one. Sorted.
pub fn diff(a: &View, b: &View) -> Vec<NodeId> {
    let _ = (a, b);
    todo!()
}

/// A new view equal to `view` except `node` uses `version`; its parent is
/// `view`. Err when the node is not in the view.
pub fn swap(
    view: &View,
    node: &NodeId,
    version: VersionId,
    created_ms: u64,
) -> Result<View, StoreError> {
    let _ = (view, node, version, created_ms);
    todo!()
}

/// The view `view` derives from, if any.
pub fn undo(store: &dyn Store, view: &View) -> Result<Option<View>, StoreError> {
    let _ = (store, view);
    todo!()
}

/// The chain of views from `view` back to the root, newest first.
pub fn lineage(store: &dyn Store, view: &ViewId) -> Result<Vec<View>, StoreError> {
    let _ = (store, view);
    todo!()
}
