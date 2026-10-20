//! A whole repository as a graph of snippets: every function, method, type and test as a node,
//! with its signature, contract and source, and the edges between them.
//!
//! This is a syntactic extractor, not a type checker. It finds a call by its path (following
//! `use` declarations and module nesting) and a method call by its name, so an edge found by
//! name alone is marked as only possible. It reads the source with `syn`, so it sees
//! exactly the code a reader sees.

use proc_macro2::LineColumn;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::visit::Visit;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Fn,
    Method,
    Test,
    Struct,
    Enum,
    Trait,
    Const,
    TypeAlias,
}

impl Kind {
    fn is_callable(&self) -> bool {
        matches!(self, Kind::Fn | Kind::Method | Kind::Test)
    }
    fn is_type(&self) -> bool {
        matches!(
            self,
            Kind::Struct | Kind::Enum | Kind::Trait | Kind::TypeAlias
        )
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Node {
    pub id: String,
    pub slug: String,
    pub crate_name: String,
    pub module: String,
    pub owner: Option<String>,
    pub name: String,
    pub kind: Kind,
    pub public: bool,
    pub file: String,
    pub line: usize,
    pub end_line: usize,
    pub signature: String,
    pub doc: String,
    pub source: String,
    /// 0 for a callable that calls no other callable; otherwise one more than the deepest thing
    /// it calls. This is the order a function would be filled in.
    pub level: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    /// "calls", "uses_type" or "uses_const"
    pub kind: &'static str,
    /// False when found only by a method's name.
    pub exact: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct RepoGraph {
    pub crates: Vec<String>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

pub struct SourceFile {
    pub crate_name: String,
    pub module: Vec<String>,
    pub path: String,
    pub text: String,
}

struct Raw {
    node: Node,
    calls: Vec<Vec<String>>,
    methods: Vec<String>,
    type_names: Vec<Vec<String>>,
    imports: Imports,
    in_module: Vec<String>,
}

#[derive(Clone, Default)]
struct Imports {
    aliases: HashMap<String, Vec<String>>,
    globs: Vec<Vec<String>>,
}

fn lines_of(text: &str) -> Vec<&str> {
    text.lines().collect()
}

fn slice(lines: &[&str], from: LineColumn, to: LineColumn) -> String {
    if from.line == 0 || from.line > lines.len() {
        return String::new();
    }
    let mut out = Vec::new();
    for n in from.line..=to.line.min(lines.len()) {
        let line = lines[n - 1];
        let chars: Vec<char> = line.chars().collect();
        let start = if n == from.line {
            from.column.min(chars.len())
        } else {
            0
        };
        let end = if n == to.line {
            to.column.min(chars.len())
        } else {
            chars.len()
        };
        out.push(chars[start..end.max(start)].iter().collect::<String>());
    }
    out.join("\n")
}

fn whole_lines(lines: &[&str], from: usize, to: usize) -> String {
    let block: Vec<&str> = (from..=to.min(lines.len()))
        .filter(|n| *n >= 1)
        .map(|n| lines[n - 1])
        .collect();
    let indent = block
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    block
        .iter()
        .map(|l| {
            if l.len() >= indent {
                &l[indent..]
            } else {
                l.trim_start()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn doc_of(attrs: &[syn::Attribute]) -> String {
    let mut out = Vec::new();
    for a in attrs {
        if !a.path().is_ident("doc") {
            continue;
        }
        if let syn::Meta::NameValue(nv) = &a.meta {
            if let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(s),
                ..
            }) = &nv.value
            {
                let line = s.value();
                out.push(line.strip_prefix(' ').unwrap_or(&line).to_string());
            }
        }
    }
    out.join("\n")
}

fn is_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| a.path().is_ident("test"))
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().is_ident("cfg") && a.meta.to_token_stream_string().contains("test"))
}

trait MetaText {
    fn to_token_stream_string(&self) -> String;
}

impl MetaText for syn::Meta {
    fn to_token_stream_string(&self) -> String {
        quote::ToTokens::to_token_stream(self).to_string()
    }
}

fn is_public(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Public(_))
}

fn path_segments(p: &syn::Path) -> Vec<String> {
    p.segments.iter().map(|s| s.ident.to_string()).collect()
}

fn use_tree(tree: &syn::UseTree, prefix: &mut Vec<String>, imports: &mut Imports) {
    match tree {
        syn::UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            use_tree(&p.tree, prefix, imports);
            prefix.pop();
        }
        syn::UseTree::Name(n) => {
            let mut full = prefix.clone();
            full.push(n.ident.to_string());
            imports.aliases.insert(n.ident.to_string(), full);
        }
        syn::UseTree::Rename(r) => {
            let mut full = prefix.clone();
            full.push(r.ident.to_string());
            imports.aliases.insert(r.rename.to_string(), full);
        }
        syn::UseTree::Glob(_) => imports.globs.push(prefix.clone()),
        syn::UseTree::Group(g) => {
            for t in &g.items {
                use_tree(t, prefix, imports);
            }
        }
    }
}

#[derive(Default)]
struct Scan {
    calls: Vec<Vec<String>>,
    methods: Vec<String>,
    type_names: Vec<Vec<String>>,
}

impl<'ast> Visit<'ast> for Scan {
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*call.func {
            self.calls.push(path_segments(&p.path));
        }
        for arg in &call.args {
            if let syn::Expr::Path(p) = arg {
                self.calls.push(path_segments(&p.path));
            }
        }
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        match &*call.receiver {
            syn::Expr::Path(p) if p.path.is_ident("self") => self
                .calls
                .push(vec!["Self".to_string(), call.method.to_string()]),
            _ => self.methods.push(call.method.to_string()),
        }
        for arg in &call.args {
            if let syn::Expr::Path(p) = arg {
                self.calls.push(path_segments(&p.path));
            }
        }
        syn::visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_path(&mut self, p: &'ast syn::ExprPath) {
        let segs = path_segments(&p.path);
        let constant = segs.len() == 1
            && segs[0].chars().any(|c| c.is_ascii_uppercase())
            && segs[0]
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
        if segs.len() >= 2 || constant {
            self.calls.push(segs);
        }
        syn::visit::visit_expr_path(self, p);
    }

    fn visit_type_path(&mut self, p: &'ast syn::TypePath) {
        self.type_names.push(path_segments(&p.path));
        syn::visit::visit_type_path(self, p);
    }

    /// `syn` leaves a macro's arguments as raw tokens. Most macros that matter here (`assert_eq!`,
    /// `println!`, `format!`) take comma-separated expressions, so try that.
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let parse = syn::punctuated::Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated;
        if let Ok(args) = m.parse_body_with(parse) {
            for e in &args {
                self.visit_expr(e);
            }
        }
        syn::visit::visit_macro(self, m);
    }
}

struct Collector<'a> {
    file: &'a SourceFile,
    lines: Vec<&'a str>,
    module: Vec<String>,
    imports: Imports,
    out: Vec<Raw>,
}

impl Collector<'_> {
    fn module_name(&self) -> String {
        self.module.join("::")
    }

    fn id(&self, owner: Option<&str>, name: &str) -> String {
        let mut parts = vec![self.file.crate_name.clone()];
        parts.extend(self.module.iter().cloned());
        if let Some(o) = owner {
            parts.push(o.to_string());
        }
        parts.push(name.to_string());
        parts.join("::")
    }

    fn push(&mut self, mut node: Node, scan: Scan) {
        node.module = self.module_name();
        self.out.push(Raw {
            node,
            calls: scan.calls,
            methods: scan.methods,
            type_names: scan.type_names,
            imports: self.imports.clone(),
            in_module: self.module.clone(),
        });
    }

    fn function(
        &mut self,
        owner: Option<&str>,
        attrs: &[syn::Attribute],
        vis: &syn::Visibility,
        sig: &syn::Signature,
        block: &syn::Block,
        whole: &dyn Spanned,
    ) {
        let name = sig.ident.to_string();
        let kind = if is_test(attrs) {
            Kind::Test
        } else if owner.is_some() {
            Kind::Method
        } else {
            Kind::Fn
        };
        let start = whole.span().start();
        let end = whole.span().end();
        let sig_start = if is_public(vis) {
            vis.span().start()
        } else {
            sig.span().start()
        };
        let signature = slice(&self.lines, sig_start, sig.span().end());
        let mut scan = Scan::default();
        scan.visit_block(block);
        for input in &sig.inputs {
            scan.visit_fn_arg(input);
        }
        scan.visit_return_type(&sig.output);
        let node = Node {
            id: self.id(owner, &name),
            slug: String::new(),
            crate_name: self.file.crate_name.clone(),
            module: String::new(),
            owner: owner.map(str::to_string),
            name,
            kind,
            public: is_public(vis),
            file: self.file.path.clone(),
            line: start.line,
            end_line: end.line,
            signature,
            doc: doc_of(attrs),
            source: whole_lines(&self.lines, start.line, end.line),
            level: 0,
        };
        self.push(node, scan);
    }

    fn typed(
        &mut self,
        kind: Kind,
        name: &str,
        attrs: &[syn::Attribute],
        vis: &syn::Visibility,
        item: &dyn Spanned,
        scan_item: &dyn Fn(&mut Scan),
    ) {
        let start = item.span().start();
        let end = item.span().end();
        let mut scan = Scan::default();
        scan_item(&mut scan);
        let source = whole_lines(&self.lines, start.line, end.line);
        let signature = source
            .lines()
            .filter(|l| !l.trim_start().starts_with("///") && !l.trim_start().starts_with("#["))
            .collect::<Vec<_>>()
            .join("\n");
        let node = Node {
            id: self.id(None, name),
            slug: String::new(),
            crate_name: self.file.crate_name.clone(),
            module: String::new(),
            owner: None,
            name: name.to_string(),
            kind,
            public: is_public(vis),
            file: self.file.path.clone(),
            line: start.line,
            end_line: end.line,
            signature,
            doc: doc_of(attrs),
            source,
            level: 0,
        };
        self.push(node, scan);
    }

    fn items(&mut self, items: &[syn::Item]) {
        for item in items {
            if let syn::Item::Use(u) = item {
                use_tree(&u.tree, &mut Vec::new(), &mut self.imports);
            }
        }
        for item in items {
            match item {
                syn::Item::Fn(f) => self.function(None, &f.attrs, &f.vis, &f.sig, &f.block, f),
                syn::Item::Struct(s) => self.typed(
                    Kind::Struct,
                    &s.ident.to_string(),
                    &s.attrs,
                    &s.vis,
                    s,
                    &|sc| sc.visit_fields(&s.fields),
                ),
                syn::Item::Enum(e) => self.typed(
                    Kind::Enum,
                    &e.ident.to_string(),
                    &e.attrs,
                    &e.vis,
                    e,
                    &|sc| {
                        for v in &e.variants {
                            sc.visit_fields(&v.fields);
                        }
                    },
                ),
                syn::Item::Trait(t) => self.typed(
                    Kind::Trait,
                    &t.ident.to_string(),
                    &t.attrs,
                    &t.vis,
                    t,
                    &|_| {},
                ),
                syn::Item::Const(c) => self.typed(
                    Kind::Const,
                    &c.ident.to_string(),
                    &c.attrs,
                    &c.vis,
                    c,
                    &|sc| sc.visit_type(&c.ty),
                ),
                syn::Item::Type(t) => self.typed(
                    Kind::TypeAlias,
                    &t.ident.to_string(),
                    &t.attrs,
                    &t.vis,
                    t,
                    &|sc| sc.visit_type(&t.ty),
                ),
                syn::Item::Impl(imp) => {
                    let owner = match &*imp.self_ty {
                        syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
                        _ => None,
                    };
                    let Some(owner) = owner else { continue };
                    for it in &imp.items {
                        if let syn::ImplItem::Fn(m) = it {
                            self.function(Some(&owner), &m.attrs, &m.vis, &m.sig, &m.block, m);
                        }
                    }
                }
                syn::Item::Mod(m) => {
                    if let Some((_, inner)) = &m.content {
                        let saved = std::mem::take(&mut self.imports);
                        let parent_imports = saved.clone();
                        self.imports = parent_imports;
                        self.module.push(m.ident.to_string());
                        if is_cfg_test(&m.attrs) && m.ident == "tests" {
                            self.imports.globs.push(vec!["super".to_string()]);
                        }
                        self.items(inner);
                        self.module.pop();
                        self.imports = saved;
                    }
                }
                _ => {}
            }
        }
    }
}

type Index = HashMap<(String, String, Option<String>, String), usize>;

fn normalise(current: &[String], segs: &[String]) -> Option<Vec<String>> {
    let first = segs.first()?;
    match first.as_str() {
        "crate" => Some(segs[1..].to_vec()),
        "self" => Some(
            current
                .iter()
                .cloned()
                .chain(segs[1..].iter().cloned())
                .collect(),
        ),
        "super" => {
            let mut base: Vec<String> = current.to_vec();
            let mut rest = segs;
            while rest.first().map(String::as_str) == Some("super") {
                base.pop();
                rest = &rest[1..];
            }
            Some(base.into_iter().chain(rest.iter().cloned()).collect())
        }
        _ => None,
    }
}

struct Resolver<'a> {
    raws: &'a [Raw],
    index: Index,
    by_name: HashMap<(String, String), Vec<usize>>,
    methods: HashMap<(String, String), Vec<usize>>,
    types: HashMap<(String, String), Vec<usize>>,
}

impl<'a> Resolver<'a> {
    fn new(raws: &'a [Raw]) -> Self {
        let mut index = Index::new();
        let mut by_name: HashMap<(String, String), Vec<usize>> = HashMap::new();
        let mut methods: HashMap<(String, String), Vec<usize>> = HashMap::new();
        let mut types: HashMap<(String, String), Vec<usize>> = HashMap::new();
        for (i, r) in raws.iter().enumerate() {
            let n = &r.node;
            index.insert(
                (
                    n.crate_name.clone(),
                    n.module.clone(),
                    n.owner.clone(),
                    n.name.clone(),
                ),
                i,
            );
            if n.kind.is_callable() && n.owner.is_none() {
                by_name
                    .entry((n.crate_name.clone(), n.name.clone()))
                    .or_default()
                    .push(i);
            }
            if n.kind == Kind::Method || (n.kind == Kind::Test && n.owner.is_some()) {
                methods
                    .entry((n.crate_name.clone(), n.name.clone()))
                    .or_default()
                    .push(i);
            }
            if n.kind.is_type() {
                types
                    .entry((n.crate_name.clone(), n.name.clone()))
                    .or_default()
                    .push(i);
            }
        }
        Resolver {
            raws,
            index,
            by_name,
            methods,
            types,
        }
    }

    fn lookup(
        &self,
        krate: &str,
        module: &[String],
        owner: Option<&str>,
        name: &str,
    ) -> Option<usize> {
        self.index
            .get(&(
                krate.to_string(),
                module.join("::"),
                owner.map(str::to_string),
                name.to_string(),
            ))
            .copied()
    }

    fn absolute(&self, from: &Raw, segs: &[String]) -> Option<Vec<String>> {
        if let Some(abs) = normalise(&from.in_module, segs) {
            return Some(abs);
        }
        let first = segs.first()?;
        if let Some(full) = from.imports.aliases.get(first) {
            let head = normalise(&from.in_module, full).or_else(|| Some(full.clone()))?;
            return Some(head.into_iter().chain(segs[1..].iter().cloned()).collect());
        }
        None
    }

    /// A call by path: `f`, `module::f`, `Type::method`, `Self::method`, `crate::a::f`.
    fn callable(&self, from: &Raw, segs: &[String]) -> Option<usize> {
        let krate = &from.node.crate_name;
        if segs.is_empty() {
            return None;
        }
        if segs[0] == "Self" && segs.len() == 2 {
            let owner = from.node.owner.as_deref()?;
            return self.lookup(krate, &from.in_module, Some(owner), &segs[1]);
        }
        if let Some(abs) = self.absolute(from, segs) {
            let (name, rest) = abs.split_last()?;
            if let Some(i) = self.lookup(krate, rest, None, name) {
                return Some(i);
            }
            if let Some((owner, module)) = rest.split_last() {
                if let Some(i) = self.lookup(krate, module, Some(owner), name) {
                    return Some(i);
                }
            }
            return None;
        }
        if segs.len() == 1 {
            let name = &segs[0];
            if let Some(i) = self.lookup(krate, &from.in_module, None, name) {
                return Some(i);
            }
            for glob in &from.imports.globs {
                let base = normalise(&from.in_module, glob).unwrap_or_else(|| glob.clone());
                if let Some(i) = self.lookup(krate, &base, None, name) {
                    return Some(i);
                }
            }
            let candidates = self.by_name.get(&(krate.clone(), name.clone()))?;
            return (candidates.len() == 1).then(|| candidates[0]);
        }
        if segs.len() == 2 {
            let (owner, name) = (&segs[0], &segs[1]);
            if let Some(i) = self.lookup(krate, &from.in_module, Some(owner), name) {
                return Some(i);
            }
            let owners: Vec<&usize> = self
                .methods
                .get(&(krate.clone(), name.clone()))?
                .iter()
                .filter(|i| self.raws[**i].node.owner.as_deref() == Some(owner.as_str()))
                .collect();
            return (owners.len() == 1).then(|| *owners[0]);
        }
        None
    }

    fn type_node(&self, from: &Raw, segs: &[String]) -> Option<usize> {
        let krate = &from.node.crate_name;
        let name = segs.last()?;
        if segs.len() > 1 {
            if let Some(abs) = self.absolute(from, segs) {
                let (n, rest) = abs.split_last()?;
                if let Some(i) = self.lookup(krate, rest, None, n) {
                    if self.raws[i].node.kind.is_type() {
                        return Some(i);
                    }
                }
            }
        }
        if let Some(i) = self.lookup(krate, &from.in_module, None, name) {
            if self.raws[i].node.kind.is_type() {
                return Some(i);
            }
        }
        let candidates = self.types.get(&(krate.clone(), name.clone()))?;
        (candidates.len() == 1).then(|| candidates[0])
    }
}

const POSSIBLE_LIMIT: usize = 3;

/// Method names that the standard library and common crates use all the time. Without types
/// there is no telling `vec.push(x)` from the crate's own `push`, so a call to one of these
/// is never matched by name (it is still found through `self.`).
const COMMON_METHODS: &[&str] = &[
    "push",
    "pop",
    "insert",
    "remove",
    "get",
    "get_mut",
    "len",
    "is_empty",
    "iter",
    "iter_mut",
    "into_iter",
    "map",
    "filter",
    "collect",
    "clone",
    "to_string",
    "to_owned",
    "into",
    "from",
    "unwrap",
    "expect",
    "join",
    "split",
    "trim",
    "contains",
    "extend",
    "push_str",
    "lock",
    "send",
    "recv",
    "read",
    "write",
    "flush",
    "take",
    "replace",
    "entry",
    "keys",
    "values",
    "sort",
    "sort_by",
    "first",
    "last",
    "next",
    "any",
    "all",
    "find",
    "position",
    "min",
    "max",
    "sum",
    "count",
    "cloned",
    "copied",
    "or_default",
    "or_insert",
    "as_str",
    "as_ref",
    "borrow",
    "display",
    "new",
    "default",
    "load",
    "store",
    "fetch_add",
    "set",
    "add",
    "run",
    "call",
    "name",
    "kind",
    "text",
    "path",
    "id",
    "parse",
    "lines",
    "chars",
    "bytes",
    "starts_with",
    "ends_with",
    "retain",
    "drain",
    "clear",
    "extend_from_slice",
    "reverse",
    "dedup",
    "truncate",
    "append",
    "concat",
    "format",
];

/// Build the graph from already-read source files.
pub fn analyse(files: &[SourceFile]) -> Result<RepoGraph, String> {
    let mut raws: Vec<Raw> = Vec::new();
    for file in files {
        let parsed = syn::parse_file(&file.text).map_err(|e| format!("{}: {e}", file.path))?;
        let mut c = Collector {
            file,
            lines: lines_of(&file.text),
            module: file.module.clone(),
            imports: Imports::default(),
            out: Vec::new(),
        };
        c.items(&parsed.items);
        raws.extend(c.out);
    }

    let resolver = Resolver::new(&raws);
    let mut edges: BTreeSet<(usize, usize, &'static str, bool)> = BTreeSet::new();
    for (i, r) in raws.iter().enumerate() {
        for segs in &r.calls {
            if let Some(j) = resolver.callable(r, segs) {
                if j != i {
                    let kind = if raws[j].node.kind == Kind::Const {
                        "uses_const"
                    } else {
                        "calls"
                    };
                    edges.insert((i, j, kind, true));
                }
            }
        }
        for m in &r.methods {
            if COMMON_METHODS.contains(&m.as_str()) {
                continue;
            }
            let Some(candidates) = resolver
                .methods
                .get(&(r.node.crate_name.clone(), m.clone()))
            else {
                continue;
            };
            if candidates.len() <= POSSIBLE_LIMIT {
                for j in candidates {
                    if *j != i {
                        edges.insert((i, *j, "calls", false));
                    }
                }
            }
        }
        for segs in &r.type_names {
            if let Some(j) = resolver.type_node(r, segs) {
                if j != i {
                    edges.insert((i, j, "uses_type", true));
                }
            }
        }
    }

    let call_graph: Vec<Vec<usize>> = {
        let mut g = vec![Vec::new(); raws.len()];
        for (a, b, kind, _) in &edges {
            if *kind == "calls" && raws[*a].node.kind != Kind::Test {
                g[*a].push(*b);
            }
        }
        g
    };
    let mut level = vec![None::<usize>; raws.len()];
    fn depth(
        i: usize,
        g: &[Vec<usize>],
        memo: &mut Vec<Option<usize>>,
        visiting: &mut Vec<bool>,
    ) -> usize {
        if let Some(l) = memo[i] {
            return l;
        }
        if visiting[i] {
            return 0;
        }
        visiting[i] = true;
        let l = g[i]
            .iter()
            .map(|j| 1 + depth(*j, g, memo, visiting))
            .max()
            .unwrap_or(0);
        visiting[i] = false;
        memo[i] = Some(l);
        l
    }
    let mut visiting = vec![false; raws.len()];
    for i in 0..raws.len() {
        depth(i, &call_graph, &mut level, &mut visiting);
    }

    let mut taken: BTreeMap<String, usize> = BTreeMap::new();
    let mut nodes: Vec<Node> = Vec::new();
    for (i, r) in raws.iter().enumerate() {
        let mut n = r.node.clone();
        n.level = level[i].unwrap_or(0);
        let base = n.id.replace("::", "/").to_lowercase();
        let count = taken.entry(base.clone()).or_insert(0);
        *count += 1;
        n.slug = if *count == 1 {
            base
        } else {
            format!("{base}-{count}")
        };
        nodes.push(n);
    }
    let mut out_edges: Vec<Edge> = edges
        .into_iter()
        .map(|(a, b, kind, exact)| Edge {
            from: nodes[a].id.clone(),
            to: nodes[b].id.clone(),
            kind,
            exact,
        })
        .collect();
    out_edges.sort_by(|a, b| (&a.from, &a.to, a.kind).cmp(&(&b.from, &b.to, b.kind)));
    let mut crates: Vec<String> = files.iter().map(|f| f.crate_name.clone()).collect();
    crates.sort();
    crates.dedup();
    Ok(RepoGraph {
        crates,
        nodes,
        edges: out_edges,
    })
}

fn module_of(src: &Path, file: &Path) -> Vec<String> {
    let rel = file.strip_prefix(src).unwrap_or(file).with_extension("");
    let mut parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_string())
        .collect();
    if parts.last().map(String::as_str) == Some("lib")
        || parts.last().map(String::as_str) == Some("mod")
    {
        parts.pop();
    }
    parts
}

fn crate_name(dir: &Path) -> String {
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default();
    let mut in_package = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
        } else if in_package {
            if let Some(rest) = t.strip_prefix("name") {
                if let Some(v) = rest.trim_start().strip_prefix('=') {
                    return v.trim().trim_matches('"').to_string();
                }
            }
        }
    }
    dir.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.path());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Read the `src` directory of each crate and build the graph. File paths in the result are
/// relative to `root`, so they can be turned into links into the repository.
pub fn build(crate_dirs: &[PathBuf], root: &Path) -> Result<RepoGraph, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", root.display()))?;
    let mut files = Vec::new();
    for dir in crate_dirs {
        let dir = &dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = crate_name(dir);
        let src = dir.join("src");
        let mut paths = Vec::new();
        rust_files(&src, &mut paths);
        for p in paths {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            files.push(SourceFile {
                crate_name: name.clone(),
                module: module_of(&src, &p),
                path: p.strip_prefix(&root).unwrap_or(&p).display().to_string(),
                text,
            });
        }
    }
    analyse(&files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(module: &[&str], text: &str) -> SourceFile {
        SourceFile {
            crate_name: "demo".into(),
            module: module.iter().map(|s| s.to_string()).collect(),
            path: format!("src/{}.rs", module.join("/")),
            text: text.into(),
        }
    }

    fn graph(files: &[SourceFile]) -> RepoGraph {
        analyse(files).unwrap()
    }

    fn has_edge(g: &RepoGraph, from: &str, to: &str, kind: &str) -> bool {
        g.edges
            .iter()
            .any(|e| e.from == from && e.to == to && e.kind == kind)
    }

    fn node<'a>(g: &'a RepoGraph, id: &str) -> &'a Node {
        g.nodes.iter().find(|n| n.id == id).unwrap_or_else(|| {
            panic!(
                "no node {id}: {:?}",
                g.nodes.iter().map(|n| &n.id).collect::<Vec<_>>()
            )
        })
    }

    #[test]
    fn items_get_kind_signature_contract_source_and_location() {
        let g = graph(&[file(&["geo"], "/// Area of a square.\n/// Second line.\npub fn area(side: f64) -> f64 {\n    side * side\n}\n\nstruct Point { x: f64 }\n")]);
        let f = node(&g, "demo::geo::area");
        assert_eq!(f.kind, Kind::Fn);
        assert_eq!(f.signature, "pub fn area(side: f64) -> f64");
        assert_eq!(f.doc, "Area of a square.\nSecond line.");
        assert!(f.source.contains("side * side"));
        assert_eq!((f.line, f.end_line), (1, 5));
        assert!(f.public);
        assert_eq!(f.module, "geo");
        let s = node(&g, "demo::geo::Point");
        assert_eq!(s.kind, Kind::Struct);
        assert!(!s.public);
    }

    #[test]
    fn methods_belong_to_their_type_and_a_trait_impl_method_too() {
        let g = graph(&[file(&["a"], "struct T;\nimpl T { pub fn new() -> T { T } }\ntrait G { fn g(&self); }\nimpl G for T { fn g(&self) {} }\n")]);
        assert_eq!(node(&g, "demo::a::T::new").kind, Kind::Method);
        assert_eq!(node(&g, "demo::a::T::g").owner.as_deref(), Some("T"));
    }

    #[test]
    fn a_call_in_the_same_module_and_through_a_path_and_an_import_is_found() {
        let g = graph(&[
            file(&["util"], "pub fn helper() -> u32 { 1 }\n"),
            file(&["a"], "use crate::util::helper;\nfn one() -> u32 { helper() }\nfn two() -> u32 { crate::util::helper() }\nfn three() -> u32 { one() }\n"),
        ]);
        assert!(has_edge(&g, "demo::a::one", "demo::util::helper", "calls"));
        assert!(has_edge(&g, "demo::a::two", "demo::util::helper", "calls"));
        assert!(has_edge(&g, "demo::a::three", "demo::a::one", "calls"));
        assert!(!has_edge(&g, "demo::a::one", "demo::a::two", "calls"));
    }

    #[test]
    fn super_and_self_and_type_paths_are_followed() {
        let g = graph(&[
            file(&["a"], "pub fn root() {}\nmod inner { pub fn deep() { super::root() } }\n"),
            file(&["b"], "struct W;\nimpl W { fn make() -> W { W } fn go(&self) { Self::make(); W::make(); } }\n"),
        ]);
        assert!(has_edge(
            &g,
            "demo::a::inner::deep",
            "demo::a::root",
            "calls"
        ));
        assert!(has_edge(&g, "demo::b::W::go", "demo::b::W::make", "calls"));
    }

    #[test]
    fn a_method_call_matched_by_name_is_only_ever_possible() {
        let g = graph(&[file(
            &["m"],
            "struct A; struct B;\nimpl A { fn only(&self) {} fn dup(&self) {} }\nimpl B { fn dup(&self) {} }\nfn run(a: A) { a.only(); a.dup(); }\n",
        )]);
        let only = g.edges.iter().find(|e| e.to == "demo::m::A::only").unwrap();
        assert!(!only.exact, "no types, so even a unique name is a guess");
        let dup: Vec<&Edge> = g.edges.iter().filter(|e| e.to.ends_with("::dup")).collect();
        assert_eq!(dup.len(), 2);
        assert!(dup.iter().all(|e| !e.exact));
    }

    #[test]
    fn a_method_name_shared_by_many_types_is_not_guessed_at() {
        let types: String = (0..5)
            .map(|i| format!("struct T{i}; impl T{i} {{ fn same(&self) {{}} }}\n"))
            .collect();
        let g = graph(&[file(
            &["m"],
            &format!("{types}fn run(x: T0) {{ x.same(); }}\n"),
        )]);
        assert!(g.edges.iter().all(|e| e.kind != "calls"));
    }

    #[test]
    fn tests_are_nodes_that_reach_what_they_call_through_super_glob() {
        let g = graph(&[file(
            &["f"],
            "fn add(a: u32, b: u32) -> u32 { a + b }\n#[cfg(test)]\nmod tests {\n    use super::*;\n    #[test]\n    fn adds() { assert_eq!(add(1, 2), 3); }\n}\n",
        )]);
        let t = node(&g, "demo::f::tests::adds");
        assert_eq!(t.kind, Kind::Test);
        assert!(has_edge(
            &g,
            "demo::f::tests::adds",
            "demo::f::add",
            "calls"
        ));
    }

    #[test]
    fn calls_inside_common_macros_are_found_and_unparseable_macros_are_skipped() {
        let g = graph(&[file(
            &["m"],
            "fn a() -> u32 { 1 }\nfn b() -> u32 { 2 }\nfn c() { assert_eq!(a(), 1); println!(\"{}\", b()); let _ = vec![a(); 3]; }\n",
        )]);
        assert!(has_edge(&g, "demo::m::c", "demo::m::a", "calls"));
        assert!(has_edge(&g, "demo::m::c", "demo::m::b", "calls"));
    }

    #[test]
    fn a_standard_method_name_is_not_matched_to_the_crates_own_but_self_calls_are() {
        let g = graph(&[file(
            &["c"],
            "struct C { v: Vec<u32> }\nimpl C { fn push(&mut self, x: u32) { self.v.push(x); }\n fn add_two(&mut self) { self.push(1); } }\nfn other(v: &mut Vec<u32>) { v.push(1); }\n",
        )]);
        assert!(has_edge(
            &g,
            "demo::c::C::add_two",
            "demo::c::C::push",
            "calls"
        ));
        assert!(!has_edge(&g, "demo::c::other", "demo::c::C::push", "calls"));
        assert!(!has_edge(
            &g,
            "demo::c::C::push",
            "demo::c::C::push",
            "calls"
        ));
    }

    #[test]
    fn a_constant_is_a_use_edge_not_a_call() {
        let g = graph(&[file(
            &["k"],
            "const LIMIT: u32 = 3;\nfn f(x: u32) -> bool { x > LIMIT }\n",
        )]);
        assert!(has_edge(&g, "demo::k::f", "demo::k::LIMIT", "uses_const"));
        assert!(g.edges.iter().all(|e| e.kind != "calls"));
    }

    #[test]
    fn types_in_a_signature_are_edges_and_levels_follow_the_calls() {
        let g = graph(&[file(
            &["l"],
            "struct Cfg;\nfn leaf() {}\nfn mid() { leaf(); }\nfn top(c: Cfg) { mid(); }\n",
        )]);
        assert!(has_edge(&g, "demo::l::top", "demo::l::Cfg", "uses_type"));
        assert_eq!(node(&g, "demo::l::leaf").level, 0);
        assert_eq!(node(&g, "demo::l::mid").level, 1);
        assert_eq!(node(&g, "demo::l::top").level, 2);
    }

    #[test]
    fn recursion_does_not_loop_and_slugs_are_unique_ignoring_case() {
        let g = graph(&[file(
            &["r"],
            "fn f() { f(); }\nstruct Graph;\nfn graph() {}\n",
        )]);
        assert_eq!(node(&g, "demo::r::f").level, 0);
        let slugs: BTreeSet<String> = g.nodes.iter().map(|n| n.slug.to_lowercase()).collect();
        assert_eq!(slugs.len(), g.nodes.len());
    }

    #[test]
    fn modules_come_from_file_paths() {
        let src = Path::new("/c/src");
        assert_eq!(
            module_of(src, Path::new("/c/src/lib.rs")),
            Vec::<String>::new()
        );
        assert_eq!(module_of(src, Path::new("/c/src/fill.rs")), ["fill"]);
        assert_eq!(module_of(src, Path::new("/c/src/bin/sg.rs")), ["bin", "sg"]);
        assert_eq!(module_of(src, Path::new("/c/src/a/mod.rs")), ["a"]);
    }

    #[test]
    fn a_syntax_error_names_the_file() {
        let err = analyse(&[file(&["bad"], "fn (")]).unwrap_err();
        assert!(err.contains("src/bad.rs"), "{err}");
    }
}
