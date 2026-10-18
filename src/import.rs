use crate::model::Graph;
use std::path::Path;

/// Parse every `.rs` file under `crate_dir/src` into one graph, keeping bodies,
/// with placements relative to the crate root.
pub fn import_crate(crate_dir: &Path) -> Result<Graph, String> {
    let _ = crate_dir;
    todo!()
}

/// Write the assembled graph back, changing only files whose rendered text
/// differs from what is on disk.
pub fn write_back(crate_dir: &Path, graph: &Graph) -> std::io::Result<()> {
    let _ = (crate_dir, graph);
    todo!()
}
