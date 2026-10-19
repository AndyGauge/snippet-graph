use crate::version::{Store, View};
use std::path::Path;

/// Render `view` to `dir` as a buildable crate (assembled sources only). When
/// `commit` is set and `dir` is a git repository, commit the result with the
/// view and run ids in the message.
pub fn export_view(store: &dyn Store, view: &View, dir: &Path, commit: bool) -> Result<(), String> {
    let _ = (store, view, dir, commit);
    todo!()
}
