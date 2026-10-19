use serde_json::{json, Value};
use snippet_graph::plugin::{self, Completion, Request};
use std::time::Duration;

const USAGE: &str = "sg-provider-openai: a snippet-graph provider for an OpenAI-compatible chat endpoint

options (or the environment variable in brackets):
  --base-url URL     endpoint root, ending in /v1   [SG_OPENAI_BASE_URL, default http://localhost:8001/v1]
  --model NAME       model id; the first one the server lists if omitted   [SG_OPENAI_MODEL]
  --api-key-env VAR  name of the environment variable holding a bearer token   [SG_OPENAI_API_KEY_ENV, default OPENAI_API_KEY]
  --channels N       requests to run at once   [SG_OPENAI_CHANNELS, default 4]
  --timeout SECS     per request   [SG_OPENAI_TIMEOUT, default 900]

Run through sg:  sg spec.md ./crate --provider openai --provider-args \"--base-url http://host:8001/v1\"";

#[derive(Clone, Debug, PartialEq)]
struct Settings {
    base_url: String,
    model: Option<String>,
    api_key: Option<String>,
    channels: usize,
    timeout: Duration,
}

fn parse(args: &[String], env: &dyn Fn(&str) -> Option<String>) -> Result<Settings, String> {
    let flag = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    for a in args {
        if a.starts_with("--") && !["--base-url", "--model", "--api-key-env", "--channels", "--timeout"].contains(&a.as_str()) {
            return Err(format!("unknown option {a}\n\n{USAGE}"));
        }
    }
    let base_url = flag("--base-url")
        .or_else(|| env("SG_OPENAI_BASE_URL"))
        .unwrap_or_else(|| "http://localhost:8001/v1".to_string());
    let key_var = flag("--api-key-env")
        .or_else(|| env("SG_OPENAI_API_KEY_ENV"))
        .unwrap_or_else(|| "OPENAI_API_KEY".to_string());
    let number = |text: Option<String>, default: usize, what: &str| -> Result<usize, String> {
        match text {
            None => Ok(default),
            Some(t) => t.parse().map_err(|_| format!("{what} must be a number, got {t}")),
        }
    };
    Ok(Settings {
        base_url: base_url.trim_end_matches('/').to_string(),
        model: flag("--model").or_else(|| env("SG_OPENAI_MODEL")),
        api_key: env(&key_var).filter(|k| !k.is_empty()),
        channels: number(flag("--channels").or_else(|| env("SG_OPENAI_CHANNELS")), 4, "--channels")?.max(1),
        timeout: Duration::from_secs(number(flag("--timeout").or_else(|| env("SG_OPENAI_TIMEOUT")), 900, "--timeout")? as u64),
    })
}

fn body(model: &str, r: &Request) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = &r.system {
        messages.push(json!({ "role": "system", "content": system }));
    }
    messages.push(json!({ "role": "user", "content": r.prompt }));
    let mut b = json!({
        "model": model,
        "messages": messages,
        "max_tokens": r.max_tokens.unwrap_or(16000),
    });
    if let Some(effort) = &r.effort {
        b["reasoning_effort"] = json!(effort);
    }
    b
}

fn completion(reply: &Value) -> Result<Completion, String> {
    let text = reply["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| format!("no text in the reply: {}", reply.to_string().chars().take(200).collect::<String>()))?;
    Ok(Completion {
        text: text.to_string(),
        input_tokens: reply["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: reply["usage"]["completion_tokens"].as_u64().unwrap_or(0),
    })
}

fn request(agent: &ureq::Agent, s: &Settings, url: &str, payload: Option<&Value>) -> Result<Value, String> {
    let mut attempt = 0;
    loop {
        let mut call = if payload.is_some() { agent.post(url) } else { agent.get(url) };
        if let Some(key) = &s.api_key {
            call = call.set("Authorization", &format!("Bearer {key}"));
        }
        let result = match payload {
            Some(p) => call.send_json(p),
            None => call.call(),
        };
        match result {
            Ok(resp) => return resp.into_json::<Value>().map_err(|e| e.to_string()),
            Err(ureq::Error::Status(code, resp)) => {
                let text = resp.into_string().unwrap_or_default();
                if (code == 503 || code == 429) && attempt < 8 {
                    attempt += 1;
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
                return Err(format!("HTTP {code}: {}", text.chars().take(300).collect::<String>()));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

fn first_model(agent: &ureq::Agent, s: &Settings) -> Result<String, String> {
    let listing = request(agent, s, &format!("{}/models", s.base_url), None)?;
    listing["data"][0]["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "the server lists no models; pass --model".to_string())
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let settings = parse(&args, &|k| std::env::var(k).ok())?;
    let agent = ureq::AgentBuilder::new().timeout(settings.timeout).build();
    let model = match &settings.model {
        Some(m) => m.clone(),
        None => first_model(&agent, &settings)?,
    };
    eprintln!("sg-provider-openai: {} model {model}, {} channel(s)", settings.base_url, settings.channels);
    let url = format!("{}/chat/completions", settings.base_url);
    let channels = settings.channels;
    plugin::serve("openai", channels, move |r| {
        let reply = request(&agent, &settings, &url, Some(&body(&model, r)))?;
        completion(&reply)
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn settings_come_from_flags_then_the_environment_then_defaults() {
        let env = |k: &str| match k {
            "SG_OPENAI_MODEL" => Some("from-env".to_string()),
            "SG_OPENAI_CHANNELS" => Some("6".to_string()),
            "MY_KEY" => Some("secret".to_string()),
            _ => None,
        };
        let s = parse(&args(&["--base-url", "http://h:1/v1/", "--api-key-env", "MY_KEY"]), &env).unwrap();
        assert_eq!(s.base_url, "http://h:1/v1");
        assert_eq!(s.model.as_deref(), Some("from-env"));
        assert_eq!(s.channels, 6);
        assert_eq!(s.api_key.as_deref(), Some("secret"));
        let d = parse(&[], &|_| None).unwrap();
        assert_eq!((d.base_url.as_str(), d.channels, d.timeout.as_secs()), ("http://localhost:8001/v1", 4, 900));
        assert!(parse(&args(&["--bogus"]), &|_| None).unwrap_err().contains("unknown option"));
        assert!(parse(&args(&["--channels", "many"]), &|_| None).is_err());
    }

    #[test]
    fn the_request_body_has_the_model_messages_and_effort() {
        let r = Request {
            prompt: "write it".into(),
            system: Some("be precise".into()),
            max_tokens: Some(500),
            effort: Some("low".into()),
            ..Request::default()
        };
        let b = body("m", &r);
        assert_eq!(b["model"], "m");
        assert_eq!(b["max_tokens"], 500);
        assert_eq!(b["reasoning_effort"], "low");
        assert_eq!(b["messages"][0]["role"], "system");
        assert_eq!(b["messages"][1]["content"], "write it");
        assert!(body("m", &Request::default()).get("reasoning_effort").is_none());
    }

    #[test]
    fn a_reply_gives_text_and_token_counts_and_a_malformed_one_is_an_error() {
        let ok = json!({"choices":[{"message":{"content":"hi"}}],"usage":{"prompt_tokens":7,"completion_tokens":3}});
        assert_eq!(completion(&ok).unwrap(), Completion { text: "hi".into(), input_tokens: 7, output_tokens: 3 });
        assert!(completion(&json!({"choices":[]})).unwrap_err().contains("no text"));
    }

    /// Serves `replies` to successive connections on a local port, returning the request texts.
    fn serve_once(replies: Vec<(u16, String)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/v1", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for (code, text) in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = vec![0u8; 65536];
                let mut got = 0;
                loop {
                    let n = stream.read(&mut buf[got..]).unwrap();
                    got += n;
                    let so_far = String::from_utf8_lossy(&buf[..got]).to_string();
                    if let Some(split) = so_far.find("\r\n\r\n") {
                        let len = so_far[..split]
                            .to_lowercase()
                            .lines()
                            .find_map(|l| l.strip_prefix("content-length:").and_then(|v| v.trim().parse::<usize>().ok()))
                            .unwrap_or(0);
                        if got >= split + 4 + len {
                            break;
                        }
                    }
                    if n == 0 {
                        break;
                    }
                }
                seen.push(String::from_utf8_lossy(&buf[..got]).to_string());
                let response = format!(
                    "HTTP/1.1 {code} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
            seen
        });
        (url, handle)
    }

    fn settings(url: &str) -> Settings {
        Settings {
            base_url: url.to_string(),
            model: None,
            api_key: Some("tok".into()),
            channels: 1,
            timeout: Duration::from_secs(10),
        }
    }

    #[test]
    fn a_real_http_round_trip_sends_the_key_and_the_body_and_reads_the_answer() {
        let (url, server) = serve_once(vec![
            (200, r#"{"data":[{"id":"detected-model"}]}"#.to_string()),
            (200, r#"{"choices":[{"message":{"content":"fn x() {}"}}],"usage":{"prompt_tokens":5,"completion_tokens":2}}"#.to_string()),
        ]);
        let s = settings(&url);
        let agent = ureq::AgentBuilder::new().timeout(s.timeout).build();
        let model = first_model(&agent, &s).unwrap();
        assert_eq!(model, "detected-model");
        let r = Request { prompt: "p".into(), ..Request::default() };
        let reply = request(&agent, &s, &format!("{url}/chat/completions"), Some(&body(&model, &r))).unwrap();
        assert_eq!(completion(&reply).unwrap().text, "fn x() {}");
        let seen = server.join().unwrap();
        assert!(seen[0].starts_with("GET /v1/models"), "{}", seen[0]);
        assert!(seen[1].starts_with("POST /v1/chat/completions"));
        assert!(seen[1].to_lowercase().contains("authorization: bearer tok"));
        assert!(seen[1].contains("\"model\":\"detected-model\""));
    }

    #[test]
    fn an_error_status_is_reported_with_its_code_and_text() {
        let (url, server) = serve_once(vec![(400, r#"{"error":"bad request"}"#.to_string())]);
        let s = settings(&url);
        let agent = ureq::AgentBuilder::new().timeout(s.timeout).build();
        let err = request(&agent, &s, &format!("{url}/chat/completions"), Some(&json!({}))).unwrap_err();
        assert!(err.starts_with("HTTP 400"), "{err}");
        assert!(err.contains("bad request"));
        server.join().unwrap();
    }
}
