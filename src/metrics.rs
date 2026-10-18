use crate::model::NodeId;
use std::collections::BTreeMap;

#[derive(Debug, Default)]
pub struct RunMetrics {
    pub retries: BTreeMap<NodeId, usize>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub wall_ms: u64,
}

impl RunMetrics {
    pub fn record_retry(&mut self, id: &NodeId) {
        let _ = id;
        todo!()
    }

    pub fn to_json(&self) -> String {
        todo!()
    }
}
