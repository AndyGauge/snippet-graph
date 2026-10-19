.PHONY: build test lint check site site-data site-dev site-clean

build:
	cargo build --workspace

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

check: lint test

site-data:
	mkdir -p site/data site/static/graph
	cargo run --quiet --bin sg -- graph --out site/data/graph.json --root . . sg-provider-openai
	cp site/data/graph.json site/static/graph/graph.json
	printf '{"url": "https://github.com/andygauge/snippet-graph", "commit": "%s"}\n' "$$(git rev-parse HEAD 2>/dev/null || echo main)" > site/data/repo.json

site:
	./site/build.sh

site-dev: site-data
	cd site/web && { [ -d node_modules ] || npm ci; } && npm run build
	cd site && hugo server --disableFastRender

site-clean:
	rm -rf site/public site/resources site/static/islands site/static/graph site/data/graph.json
