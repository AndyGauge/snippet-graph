use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Use,
    Type,
    Const,
    Function,
    Method,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placement {
    pub file: PathBuf,
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    /// Item text up to the body for Function/Method (`pub fn f(x: u32) -> u32`);
    /// the full item text for every other kind.
    pub signature: String,
    /// Block text including braces. `None` means unfilled and renders as `{ todo!() }`.
    pub body: Option<String>,
    /// For Method nodes, the impl header they belong to (`impl Foo`, `impl Trait for Foo`).
    pub impl_header: Option<String>,
    pub placement: Placement,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: BTreeMap<NodeId, Node>,
    /// `uses[a]` contains `b` when `a` depends on `b`.
    pub uses: BTreeMap<NodeId, BTreeSet<NodeId>>,
}

impl Node {
    pub fn is_filled(&self) -> bool {
        match self.kind {
            NodeKind::Function | NodeKind::Method => self.body.is_some(),
            _ => true,
        }
    }

    pub fn render(&self) -> String {
        match self.kind {
            NodeKind::Function | NodeKind::Method => {
                let body = self.body.as_deref().unwrap_or("{ todo!() }");
                format!("{} {}", self.signature, body)
            }
            _ => self.signature.clone(),
        }
    }
}

impl Graph {
    pub fn insert(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_use(&mut self, from: &NodeId, to: &NodeId) {
        self.uses
            .entry(from.clone())
            .or_default()
            .insert(to.clone());
    }

    pub fn dependencies(&self, id: &NodeId) -> impl Iterator<Item = &NodeId> {
        self.uses.get(id).into_iter().flatten()
    }
}
