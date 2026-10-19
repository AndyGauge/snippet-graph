use crate::assemble::Assembled;
use crate::model::NodeId;
use crate::version::BenchResult;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct BenchConfig {
    pub runs: u32,
    pub noise_pct: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    Faster,
    Slower,
    Same,
}

/// Write the files, build in release mode, and run each bench node (all when
/// `only` is empty) `cfg.runs` times, reporting median and spread. A bench node
/// is a `#[test]`-attributed function that prints `BENCH <ns>` after timing its
/// workload.
pub fn run_benches(
    crate_dir: &Path,
    assembled: &Assembled,
    only: &[NodeId],
    cfg: &BenchConfig,
) -> std::io::Result<Vec<BenchResult>> {
    let _ = (crate_dir, assembled, only, cfg);
    todo!()
}

/// `Faster` only when the variant's median is below the baseline's by more than
/// `noise_pct` percent and more than both spreads; `Slower` symmetrically.
pub fn compare(baseline: &BenchResult, variant: &BenchResult, noise_pct: f64) -> Comparison {
    let _ = (baseline, variant, noise_pct);
    todo!()
}
