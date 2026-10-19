import test from "node:test";
import assert from "node:assert/strict";
import { NODES, levels, suspects, stepsFor } from "./graph.js";
import { analyse, EXAMPLES, parameterCount } from "./duties.js";
import runs from "../../../data/runs.json" with { type: "json" };
import throughput from "../../../data/throughput.json" with { type: "json" };

test("levels: nothing used is level 0, each caller one deeper", () => {
  const L = levels(NODES);
  assert.deepEqual([L.net_sales, L.order_balance, L.counts_toward_month, L.order_commission], [0, 0, 0, 0]);
  assert.equal(L.rep_statement, 1);
  assert.equal(L.month_summary, 2);
});

test("levels rejects a cycle", () => {
  assert.throws(() => levels([{ id: "a", uses: ["b"] }, { id: "b", uses: ["a"] }]), /cycle/);
});

test("blame: a function cleared by its own passing test is not retried", () => {
  const tests = [
    { passed: false, reaches: ["month_summary", "rep_statement", "net_sales"] },
    { passed: true, reaches: ["rep_statement", "net_sales"] },
  ];
  assert.deepEqual(suspects(NODES, tests), ["month_summary"]);
});

test("blame: a failure that is only a todo!() elsewhere blames nobody", () => {
  const tests = [{ passed: false, todo: true, reaches: ["month_summary", "net_sales"] }];
  assert.deepEqual(suspects(NODES, tests), []);
});

test("the easy path has fewer steps than the hard path and both end in a handoff", () => {
  const easy = stepsFor(false), hard = stepsFor(true);
  assert.ok(easy.length < hard.length);
  assert.equal(easy.at(-1).title, "Handoff");
  assert.equal(hard.at(-1).states.month_summary, "spent");
  assert.equal(hard.at(-1).attempts, 10);
});

test("the review check flags month_summary's contract and not net_sales'", () => {
  const big = analyse(EXAMPLES.month_summary);
  assert.equal(big.flagged, true);
  assert.equal(big.bullets, 4);
  assert.match(big.reasons[0], /separate duties/);
  assert.equal(analyse(EXAMPLES.net_sales).flagged, false);
  assert.equal(analyse(EXAMPLES["after the split"]).flagged, false);
});

test("bullets under # Arguments are not duties", () => {
  const r = analyse("/// Adds.\n///\n/// # Arguments\n///\n/// * a\n/// * b\n/// * c\n/// * d\n/// * e\npub fn add(a: u32, b: u32) -> u32");
  assert.equal(r.flagged, false);
  assert.equal(r.bullets, 0);
});

test("parameters: counted at the top level only", () => {
  assert.equal(parameterCount("pub fn f() -> u32"), 0);
  assert.equal(parameterCount("pub fn f(a: HashMap<u32, u32>) -> u32"), 1);
  assert.equal(analyse("pub fn f(a: u32, b: u32, c: u32, d: u32, e: u32, g: Vec<(u32, u32)>) -> u32").reasons[0], "6 parameters (limit 5)");
});

test("data: every run and series is complete", () => {
  assert.equal(runs.runs.length, 5);
  for (const r of runs.runs) assert.ok(["passed", "failed"].includes(r.result));
  const n = throughput.streams.length;
  for (const s of [...throughput.series, ...throughput.also]) assert.equal(s.total.length, n, s.name);
  for (const s of throughput.series) assert.equal(s.perStream.length, n, s.name);
});

import { index, neighbours, search, levelCounts, modules, visit, firstLine } from "./walk.js";

const tiny = {
  nodes: [
    { id: "c::a::top", name: "top", kind: "fn", module: "a", doc: "Does the whole thing.", level: 2, crate_name: "c" },
    { id: "c::a::mid", name: "mid", kind: "fn", module: "a", doc: "", level: 1, crate_name: "c" },
    { id: "c::a::leaf", name: "leaf", kind: "fn", module: "b", doc: "Returns a number.", level: 0, crate_name: "c" },
    { id: "c::a::T::go", name: "go", kind: "method", module: "a", doc: "", level: 0, crate_name: "c" },
    { id: "c::a::tests::checks", name: "checks", kind: "test", module: "a::tests", doc: "", level: 0, crate_name: "c" },
    { id: "c::a::Cfg", name: "Cfg", kind: "struct", module: "a", doc: "", level: 0, crate_name: "c" },
  ],
  edges: [
    { from: "c::a::top", to: "c::a::mid", kind: "calls", exact: true },
    { from: "c::a::mid", to: "c::a::leaf", kind: "calls", exact: true },
    { from: "c::a::mid", to: "c::a::T::go", kind: "calls", exact: false },
    { from: "c::a::tests::checks", to: "c::a::mid", kind: "calls", exact: true },
    { from: "c::a::top", to: "c::a::Cfg", kind: "uses_type", exact: true },
    { from: "c::a::top", to: "c::gone", kind: "calls", exact: true },
  ],
};

test("walk: neighbours are grouped, tests are kept apart, and dangling edges are dropped", () => {
  const ix = index(tiny);
  const m = neighbours(ix, "c::a::mid");
  assert.deepEqual(m.calls.map((x) => x.id), ["c::a::leaf"]);
  assert.deepEqual(m.possible.map((x) => x.id), ["c::a::T::go"]);
  assert.deepEqual(m.calledBy.map((x) => x.id), ["c::a::top"]);
  assert.deepEqual(m.tests.map((x) => x.id), ["c::a::tests::checks"]);
  const t = neighbours(ix, "c::a::top");
  assert.deepEqual(t.types.map((x) => x.id), ["c::a::Cfg"]);
  assert.equal(t.calls.length, 1, "the edge to a node that is not in the graph is ignored");
});

test("walk: search ranks an exact name above a prefix above a contract match", () => {
  const ix = index(tiny);
  assert.deepEqual(search(ix, "leaf").nodes.map((n) => n.name), ["leaf"]);
  assert.equal(search(ix, "number").nodes[0].name, "leaf");
  assert.equal(search(ix, "m").nodes[0].name, "mid");
  assert.equal(search(ix, "").total, 6);
  assert.deepEqual(search(ix, "", { kinds: new Set(["test"]) }).nodes.map((n) => n.name), ["checks"]);
  assert.deepEqual(search(ix, "", { module: "b" }).nodes.map((n) => n.name), ["leaf"]);
});

test("walk: depth counts, modules and the trail", () => {
  assert.deepEqual(levelCounts(tiny), [2, 1, 1]);
  assert.equal(modules(tiny)[0].module, "a");
  assert.deepEqual(visit(["x", "y", "z"], "y"), ["x", "z", "y"]);
  assert.equal(visit(["1", "2", "3"], "4", 3).length, 3);
  assert.equal(firstLine("\n  first\nsecond"), "  first");
});
