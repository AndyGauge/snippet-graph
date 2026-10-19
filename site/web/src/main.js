import { mount } from "svelte";
import GraphPlayground from "./islands/GraphPlayground.svelte";
import DutyChecker from "./islands/DutyChecker.svelte";
import GraphExplorer from "./islands/GraphExplorer.svelte";
import RunsChart from "./islands/RunsChart.svelte";
import ThroughputChart from "./islands/ThroughputChart.svelte";

const islands = { GraphExplorer, GraphPlayground, DutyChecker, RunsChart, ThroughputChart };

for (const el of document.querySelectorAll("[data-island]")) {
  const Component = islands[el.dataset.island];
  if (!Component) continue;
  let props = {};
  try {
    props = el.dataset.props ? { data: JSON.parse(el.dataset.props) } : {};
  } catch {
    continue;
  }
  el.replaceChildren();
  mount(Component, { target: el, props });
}
