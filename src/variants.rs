use crate::bench::BenchConfig;
use crate::fill::Generator;
use crate::model::{Graph, NodeId, TestReport};
use crate::version::{BenchResult, RunId, Store, StoreError, VersionId, View};

pub struct Evaluation {
    pub tests: TestReport,
    pub benches: Vec<BenchResult>,
}

pub struct Candidate {
    pub version: VersionId,
    pub passed: bool,
    pub bench: Option<BenchResult>,
}

pub struct TournamentReport {
    pub baseline: Option<BenchResult>,
    pub candidates: Vec<Candidate>,
    pub winner: Option<VersionId>,
    pub view: Option<View>,
}

/// Generate `n` variants of `node` in parallel (same signature), store each as a
/// version, evaluate each swapped into `base` (tests then benches), and pick the
/// fastest passing variant that beats the baseline beyond the noise threshold.
/// The winning view gets a composed evaluation before it is returned; a winner
/// that fails it is discarded. Every candidate is recorded in the store
/// whether it passes or not.
#[allow(clippy::too_many_arguments)]
pub fn tournament(
    store: &mut dyn Store,
    base: &View,
    node: &NodeId,
    n: usize,
    spec: &str,
    generators: &mut [Box<dyn Generator + Send>],
    evaluate: &mut dyn FnMut(&Graph) -> Evaluation,
    cfg: &BenchConfig,
    run: &RunId,
) -> Result<TournamentReport, StoreError> {
    let _ = (store, base, node, n, spec, generators, evaluate, cfg, run);
    todo!()
}
