// Walking the repository's graph: what a node uses, what uses it, which tests reach it.

export function index(graph) {
  const byId = new Map(graph.nodes.map((n) => [n.id, n]));
  const out = new Map();
  const into = new Map();
  for (const e of graph.edges) {
    if (!byId.has(e.from) || !byId.has(e.to)) continue;
    if (!out.has(e.from)) out.set(e.from, []);
    if (!into.has(e.to)) into.set(e.to, []);
    out.get(e.from).push(e);
    into.get(e.to).push(e);
  }
  return { byId, out, into, nodes: graph.nodes };
}

const byName = (ix) => (a, b) => ix.byId.get(a.id).id.localeCompare(ix.byId.get(b.id).id);

// Everything one step from a node, grouped the way the page shows it.
export function neighbours(ix, id) {
  const outgoing = ix.out.get(id) ?? [];
  const incoming = ix.into.get(id) ?? [];
  const pick = (edges, side, pred) =>
    edges
      .filter(pred)
      .map((e) => ({ id: e[side], exact: e.exact, kind: e.kind }))
      .sort(byName(ix));
  const isTest = (e, side) => ix.byId.get(e[side]).kind === "test";
  return {
    calls: pick(outgoing, "to", (e) => e.kind === "calls" && e.exact),
    possible: pick(outgoing, "to", (e) => e.kind === "calls" && !e.exact),
    types: pick(outgoing, "to", (e) => e.kind === "uses_type"),
    constants: pick(outgoing, "to", (e) => e.kind === "uses_const"),
    calledBy: pick(incoming, "from", (e) => e.kind === "calls" && !isTest(e, "from")),
    tests: pick(incoming, "from", (e) => e.kind === "calls" && isTest(e, "from")),
    usedAsType: pick(incoming, "from", (e) => e.kind === "uses_type"),
  };
}

// Name matches first, then module and contract text. Tests are hidden unless asked for.
export function search(ix, query, { kinds = null, module = "", limit = 60 } = {}) {
  const q = query.trim().toLowerCase();
  const scored = [];
  for (const n of ix.nodes) {
    if (kinds && !kinds.has(n.kind)) continue;
    if (module && n.module !== module) continue;
    let score = 0;
    if (!q) score = 1;
    else {
      const name = n.name.toLowerCase();
      if (name === q) score = 100;
      else if (name.startsWith(q)) score = 60;
      else if (name.includes(q)) score = 40;
      else if (n.id.toLowerCase().includes(q)) score = 20;
      else if (n.doc.toLowerCase().includes(q)) score = 5;
    }
    if (score > 0) scored.push([score, n]);
  }
  scored.sort((a, b) => b[0] - a[0] || a[1].id.localeCompare(b[1].id));
  return { total: scored.length, nodes: scored.slice(0, limit).map((s) => s[1]) };
}

// How many callables sit at each depth: the order a function would be filled in.
export function levelCounts(graph) {
  const counts = [];
  for (const n of graph.nodes) {
    if (n.kind !== "fn" && n.kind !== "method") continue;
    counts[n.level] = (counts[n.level] ?? 0) + 1;
  }
  return Array.from(counts, (c) => c ?? 0);
}

export function modules(graph) {
  const m = new Map();
  for (const n of graph.nodes) {
    const key = `${n.crate_name}|${n.module}`;
    const e = m.get(key) ?? { crate: n.crate_name, module: n.module, nodes: 0, tests: 0 };
    e.nodes += 1;
    if (n.kind === "test") e.tests += 1;
    m.set(key, e);
  }
  return [...m.values()].sort((a, b) => b.nodes - a.nodes);
}

// A trail of visited nodes: revisiting moves a node to the end instead of repeating it.
export function visit(trail, id, max = 8) {
  const next = trail.filter((t) => t !== id);
  next.push(id);
  return next.slice(-max);
}

export function firstLine(text) {
  const line = (text ?? "").split("\n").find((l) => l.trim()) ?? "";
  return line.length > 140 ? `${line.slice(0, 137)}...` : line;
}
