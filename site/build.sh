#!/bin/sh
# Read this repository into a graph, build the interactive parts, then the site. Output is in public/.
set -e
cd "$(dirname "$0")"
root=..
mkdir -p data static/graph
cargo run --quiet --manifest-path "$root/Cargo.toml" --bin sg -- graph --out data/graph.json --root "$root" "$root" "$root/sg-provider-openai"
cp data/graph.json static/graph/graph.json
commit=$(git -C "$root" rev-parse HEAD 2>/dev/null || echo main)
printf '{"url": "https://github.com/andygauge/snippet-graph", "commit": "%s"}\n' "$commit" > data/repo.json
cd web
[ -d node_modules ] || npm ci
npm test
npm run build
cd ..
hugo --minify
