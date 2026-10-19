// The commission module, as the tool saw it: which function uses which.
export const NODES = [
  { id: "counts_toward_month", uses: [] },
  { id: "net_sales", uses: [] },
  { id: "order_balance", uses: [] },
  { id: "order_commission", uses: [] },
  { id: "rep_statement", uses: ["counts_toward_month", "net_sales", "order_balance", "order_commission"] },
  { id: "month_summary", uses: ["rep_statement", "counts_toward_month"] },
];

// Level 0 uses no other function; otherwise one more than the deepest thing it uses.
export function levels(nodes) {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const memo = new Map();
  const level = (id, seen = new Set()) => {
    if (memo.has(id)) return memo.get(id);
    if (seen.has(id)) throw new Error(`cycle through ${id}`);
    seen.add(id);
    const uses = byId.get(id).uses;
    const l = uses.length === 0 ? 0 : 1 + Math.max(...uses.map((u) => level(u, new Set(seen))));
    memo.set(id, l);
    return l;
  };
  return Object.fromEntries(nodes.map((n) => [n.id, level(n.id)]));
}

// Functions to retry: reached by a failing test, and not cleared by a passing test of their own.
// A test that fails by reaching a todo!() blames nobody; the unfilled function gets filled anyway.
export function suspects(nodes, tests) {
  const reach = new Map();
  for (const t of tests) {
    if (!t.passed && t.todo) continue;
    for (const id of t.reaches) {
      const r = reach.get(id) ?? { passing: 0, failing: 0 };
      if (t.passed) r.passing += 1;
      else r.failing += 1;
      reach.set(id, r);
    }
  }
  return [...reach.entries()]
    .filter(([, r]) => r.failing > 0 && r.passing === 0)
    .map(([id]) => id);
}

export function stepsFor(hard) {
  const L = levels(NODES);
  const all = NODES.map((n) => n.id);
  const at = (l) => all.filter((id) => L[id] === l);
  const states = (f) => Object.fromEntries(all.map((id) => [id, f(id)]));
  const steps = [
    { title: "Start from a spec", text: "A spec says what each function must do. Nothing is written yet.", states: states(() => "none") },
    { title: "Skeleton", text: "Names, signatures and a contract for each function. Every body is todo!(). It has to compile, so the compiler checks the whole design before any logic exists.", states: states(() => "todo") },
    { title: "Tests first", text: "Tests are written from the spec before any body, and each must fail on the skeleton. A test that passes against todo!() proves nothing and is thrown out.", states: states(() => "red") },
    { title: "Fill level 0", text: `${at(0).length} functions use no other function, so they are filled in parallel, each seeing only the signatures of what it uses.`, states: states((id) => (L[id] === 0 ? "passing" : "red")) },
    { title: "Fill level 1", text: "rep_statement uses all four, and sees their signatures and contracts, not their bodies.", states: states((id) => (L[id] <= 1 ? "passing" : "red")) },
  ];
  if (!hard) {
    steps.push({ title: "Fill level 2", text: "month_summary passes its tests first time.", states: states(() => "passing") });
  } else {
    steps.push(
      { title: "Fill level 2", text: "month_summary fails its tests. Its contract bundles a filter, an ordering rule, four totals and an exclusion.", states: states((id) => (id === "month_summary" ? "failing" : "passing")) },
      { title: "Blame", text: "Every function under month_summary is reached by the failing tests, but their own tests pass, so they are cleared. Only month_summary is retried, with the failure text in its prompt.", states: states((id) => (id === "month_summary" ? "retry" : "passing")), attempts: 2 },
      { title: "Repair, within a budget", text: "Each function gets 10 attempts for the whole run. A repeat of an earlier answer is a free rollback and is not tested again. A function that spends its budget stops being retried.", states: states((id) => (id === "month_summary" ? "retry" : "passing")), attempts: 7, rollbacks: 2 },
    );
  }
  steps.push({
    title: "Handoff",
    text: hard
      ? "month_summary is out of budget, so the last version that compiled goes back in (or todo!() if none did). A report lists every function's status, depth, attempts and last error, and names where to start."
      : "A report lists every function's status, depth, attempts and last error, and names where to start. Each attempt along the way is a commit in a git repository.",
    states: states((id) => (hard && id === "month_summary" ? "spent" : "passing")),
    attempts: hard ? 10 : undefined,
    rollbacks: hard ? 2 : undefined,
  });
  return steps;
}
