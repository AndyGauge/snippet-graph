// A port of the skeleton review's cheap check (snippet-graph/src/review.rs): does a function's
// contract name more separate duties than one function should carry?
const HEADINGS = ["# arguments", "# returns", "# errors", "# panics", "# examples", "# safety"];
const DUTY_WORDS = [
  ["ordering", ["ordered", "sorted", "ascending", "descending", "in order of"]],
  ["exclusion", ["ignored", "skips", "skipped", "excluded", "excluding", "except", "not counted"]],
  ["filtering", ["only if", "at least one", "that count", "which count", "whose", "filter"]],
  ["empty case", ["returns zero", "zero when", "empty"]],
];
export const LIMITS = { duties: 3, longWords: 60, longDuties: 2, params: 5 };

export function parameterCount(code) {
  const open = code.indexOf("(");
  if (open < 0) return 0;
  let depth = 0, commas = 0, any = false;
  for (const c of code.slice(open)) {
    if ("(<[".includes(c)) depth += 1;
    else if (")>]".includes(c)) { depth -= 1; if (depth === 0) break; }
    else if (c === "," && depth === 1) commas += 1;
    else if (depth >= 1 && !/\s/.test(c)) any = true;
  }
  return any ? commas + 1 : 0;
}

export function analyse(source) {
  const docs = [];
  const code = [];
  for (const raw of source.split("\n")) {
    const line = raw.trim();
    if (line.startsWith("///")) docs.push(line.slice(3).trim().toLowerCase());
    else if (line.startsWith("//") || line === "") continue;
    else code.push(line);
  }
  const main = [];
  for (const t of docs) {
    if (HEADINGS.some((h) => t.startsWith(h))) break;
    main.push(t);
  }
  const bullets = main.filter((t) => /^(\*|-) /.test(t) || /^\d+[.)] /.test(t)).length;
  const joined = main.join(" ");
  const words = joined.split(/\s+/).filter(Boolean).length;
  const found = DUTY_WORDS.filter(([, ws]) => ws.some((w) => joined.includes(w))).map(([n]) => n);
  const duties = bullets + found.length;
  const reasons = [];
  if (duties > LIMITS.duties || (words >= LIMITS.longWords && duties >= LIMITS.longDuties)) {
    const parts = [];
    if (bullets > 0) parts.push(`${bullets} listed results`);
    parts.push(...found);
    reasons.push(`contract names ${duties} separate duties (${parts.join(", ")}) in ${words} words`);
  }
  const params = parameterCount(code.join(" "));
  if (params > LIMITS.params) reasons.push(`${params} parameters (limit ${LIMITS.params})`);
  return { bullets, words, found, duties, params, reasons, flagged: reasons.length > 0 };
}

export const EXAMPLES = {
  "month_summary": `/// Produces a month-wide summary across all representatives specified in \`rates\`.
/// For each \`(rep_id, rate)\` pair, a \`RepStatement\` is generated if that representative
/// has at least one order that counts toward the month. Statements are ordered by
/// ascending \`rep_id\`. The summary aggregates:
/// * \`orders\`: total counted orders across all included representatives,
/// * \`net_sales\`: sum of net sales across those statements,
/// * \`commission\`: sum of commissions across those statements,
/// * \`unpaid_orders\`: number of counted orders whose payments sum to exactly zero
///   (including orders with no payments).
///
/// Orders belonging to representatives not listed in \`rates\` are ignored entirely.
///
/// # Arguments
///
/// * \`orders\` - Slice of all orders to evaluate.
/// * \`rates\` - Slice of \`(rep_id, commission_rate)\` tuples.
pub fn month_summary(orders: &[OrderRecord], rates: &[(u32, f64)], year: i32, month: u32) -> MonthSummary`,
  "net_sales": `/// Sum of the line totals of the order after discount, before tax and shipping.
/// Returns \`Decimal::ZERO\` when the order has no lines.
pub fn net_sales(order: &OrderRecord) -> Decimal`,
  "after the split": `/// Counts the orders in the month, for reps in \`rates\`, whose payments add up to exactly zero.
/// An order with no payments counts.
fn count_unpaid_orders(orders: &[OrderRecord], rates: &[(u32, f64)], year: i32, month: u32) -> usize`,
};
