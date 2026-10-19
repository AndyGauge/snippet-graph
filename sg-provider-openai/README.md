# sg-provider-openai

A [snippet-graph](https://github.com/andygauge/snippet-graph) provider for any OpenAI-compatible chat endpoint: `llama.cpp`'s `llama-server`, vLLM, or a hosted API.

```sh
cargo install snippet-graph sg-provider-openai
sg spec.md ./my-crate --provider openai \
   --provider-args "--base-url http://localhost:8001/v1"
```

Options (or the environment variable shown): `--base-url` (`SG_OPENAI_BASE_URL`), `--model` (`SG_OPENAI_MODEL`, the first model the server lists if omitted), `--api-key-env` (`SG_OPENAI_API_KEY_ENV`, default `OPENAI_API_KEY`), `--channels` (`SG_OPENAI_CHANNELS`, default 4, the number of requests to run at once; set it to the server's parallel slot count), `--timeout` (`SG_OPENAI_TIMEOUT`, default 900 seconds).

`SG_REASONING_EFFORT=low|medium|high` is passed on as `reasoning_effort` for models that have it.
