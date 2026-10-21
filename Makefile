-include .env
# only when creating
# export CARGO_REGISTRY_TOKEN ?= $(PUB_NEW_CRATE_TOKEN)

.PHONY: publish publish-dry build test lint check site site-data site-dev site-clean

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

publish-dry: check
	cargo publish --dry-run -p snippet-graph --allow-dirty

publish: publish-dry
	@test -n "$$CARGO_REGISTRY_TOKEN" || { echo "no token: put PUB_NEW_CRATE_TOKEN=... in .env"; exit 1; }
	@test -z "$$(git status --porcelain)" || { echo "commit your changes first"; exit 1; }
	cargo publish -p snippet-graph
	@echo "waiting for the index to pick up snippet-graph"; sleep 30
	cargo publish -p sg-provider-openai
