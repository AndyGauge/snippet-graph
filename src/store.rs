use crate::model::NodeId;
use crate::version::{NodeVersion, Results, Store, StoreError, VersionId, View, ViewId};
use std::path::{Path, PathBuf};

/// Directory-backed store: content-addressed JSON files under
/// `versions/`, `views/` and `results/`, plus an append-only `order.log`
/// recording creation order. Writes are atomic (temp file then rename) and
/// every read verifies the content hash matches the file name.
pub struct DirStore {
    root: PathBuf,
}

impl DirStore {
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        let _ = root;
        todo!()
    }
}

impl Store for DirStore {
    fn put_version(&mut self, version: NodeVersion) -> Result<VersionId, StoreError> {
        let _ = (&self.root, version);
        todo!()
    }
    fn get_version(&self, id: &VersionId) -> Result<NodeVersion, StoreError> {
        let _ = id;
        todo!()
    }
    fn versions_of(&self, node: &NodeId) -> Result<Vec<VersionId>, StoreError> {
        let _ = node;
        todo!()
    }
    fn put_view(&mut self, view: View) -> Result<ViewId, StoreError> {
        let _ = view;
        todo!()
    }
    fn get_view(&self, id: &ViewId) -> Result<View, StoreError> {
        let _ = id;
        todo!()
    }
    fn list_views(&self) -> Result<Vec<ViewId>, StoreError> {
        todo!()
    }
    fn put_results(&mut self, results: Results) -> Result<(), StoreError> {
        let _ = results;
        todo!()
    }
    fn results_for(&self, view: &ViewId) -> Result<Option<Results>, StoreError> {
        let _ = view;
        todo!()
    }
    fn all_results(&self) -> Result<Vec<Results>, StoreError> {
        todo!()
    }
}
