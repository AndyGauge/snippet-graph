//! Model providers as separate programs.
//!
//! `sg` does not know how to reach a model. It starts a provider program and exchanges one JSON
//! object per line with it over the program's stdin and stdout. The program's stderr is passed
//! through, so a provider can log.
//!
//! ```text
//! sg       -> {"hello":1}
//! provider -> {"hello":1,"name":"registry","channels":8}
//! sg       -> {"id":7,"prompt":"...","system":"...","slot":2,"max_tokens":16000,"effort":"low"}
//! provider -> {"id":7,"text":"...","input_tokens":812,"output_tokens":240}
//! provider -> {"id":9,"error":"HTTP 503"}
//! ```
//!
//! Requests are tagged with an id, and replies may come back in any order, so one provider
//! process serves every worker at once. A line that is not valid JSON is ignored, which lets
//! a provider (or a test harness around it) print other things to stdout.

use crate::fill::Generator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const PROTOCOL: u32 = 1;
pub const PROVIDER_PREFIX: &str = "sg-provider-";
const REPLY_TIMEOUT: Duration = Duration::from_secs(1800);
const HELLO_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    /// Which parallel worker is asking, so a provider can spread workers across backends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// "low", "medium" or "high", for providers whose models have a reasoning setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reply {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub hello: u32,
    #[serde(default)]
    pub name: String,
    /// How many requests the provider can serve at once; `sg` runs that many workers by default.
    #[serde(default = "one")]
    pub channels: usize,
}

fn one() -> usize {
    1
}

/// What a provider's handler returns for one request.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Completion {
    pub text: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Run a provider: answer the handshake, then serve requests from stdin until it closes, each
/// on its own thread. A provider program is `main` plus a call to this.
pub fn serve<F>(name: &str, channels: usize, handler: F) -> std::io::Result<()>
where
    F: Fn(&Request) -> Result<Completion, String> + Send + Sync + 'static,
{
    serve_on(
        std::io::stdin().lock(),
        Arc::new(Mutex::new(std::io::stdout())),
        name,
        channels,
        handler,
    )
}

fn serve_on<R, W, F>(
    input: R,
    out: Arc<Mutex<W>>,
    name: &str,
    channels: usize,
    handler: F,
) -> std::io::Result<()>
where
    R: BufRead,
    W: Write + Send + 'static,
    F: Fn(&Request) -> Result<Completion, String> + Send + Sync + 'static,
{
    let handler = Arc::new(handler);
    let send = |out: &Arc<Mutex<W>>, line: String| {
        if let Ok(mut w) = out.lock() {
            let _ = writeln!(w, "{line}");
            let _ = w.flush();
        }
    };
    std::thread::scope(|scope| {
        for line in input.lines() {
            let Ok(line) = line else { break };
            if let Ok(h) = serde_json::from_str::<Hello>(&line) {
                let reply = Hello {
                    hello: PROTOCOL,
                    name: name.to_string(),
                    channels: channels.max(1),
                };
                send(&out, serde_json::to_string(&reply).unwrap_or_default());
                let _ = h;
                continue;
            }
            let Ok(request) = serde_json::from_str::<Request>(&line) else {
                continue;
            };
            let handler = handler.clone();
            let out = out.clone();
            scope.spawn(move || {
                let reply = match handler(&request) {
                    Ok(c) => Reply {
                        id: request.id,
                        text: Some(c.text),
                        input_tokens: c.input_tokens,
                        output_tokens: c.output_tokens,
                        ..Reply::default()
                    },
                    Err(error) => Reply {
                        id: request.id,
                        error: Some(error),
                        ..Reply::default()
                    },
                };
                if let Ok(mut w) = out.lock() {
                    let _ = writeln!(w, "{}", serde_json::to_string(&reply).unwrap_or_default());
                    let _ = w.flush();
                }
            });
        }
    });
    Ok(())
}

#[derive(Default)]
pub struct Usage {
    pub calls: AtomicU64,
    pub input: AtomicU64,
    pub output: AtomicU64,
}

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<Reply, String>>>>>;

/// A running provider program.
pub struct Plugin {
    child: Mutex<Child>,
    writer: Mutex<ChildStdin>,
    pending: Pending,
    next_id: AtomicU64,
    pub info: Hello,
    pub usage: Usage,
    effort: Option<String>,
}

impl Plugin {
    /// Start `program` with `args` and wait for its handshake.
    pub fn spawn(program: &Path, args: &[String]) -> Result<Arc<Plugin>, String> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("could not start provider {}: {e}", program.display()))?;
        let mut writer = child.stdin.take().ok_or("provider has no stdin")?;
        let stdout = child.stdout.take().ok_or("provider has no stdout")?;
        writeln!(writer, "{}", serde_json::json!({ "hello": PROTOCOL }))
            .and_then(|_| writer.flush())
            .map_err(|e| format!("could not talk to provider {}: {e}", program.display()))?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (hello_tx, hello_rx) = channel::<Hello>();
        let reader_pending = pending.clone();
        let name = program.display().to_string();
        std::thread::spawn(move || {
            let mut greeted = false;
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if !greeted {
                    if let Ok(h) = serde_json::from_str::<Hello>(&line) {
                        greeted = true;
                        let _ = hello_tx.send(h);
                        continue;
                    }
                }
                if let Ok(reply) = serde_json::from_str::<Reply>(&line) {
                    if reply.text.is_none() && reply.error.is_none() {
                        continue;
                    }
                    let tx = reader_pending
                        .lock()
                        .ok()
                        .and_then(|mut p| p.remove(&reply.id));
                    if let Some(tx) = tx {
                        let _ = tx.send(Ok(reply));
                    }
                }
            }
            if let Ok(mut p) = reader_pending.lock() {
                for (_, tx) in p.drain() {
                    let _ = tx.send(Err(format!("provider {name} exited")));
                }
            }
        });
        let info = hello_rx.recv_timeout(HELLO_TIMEOUT).map_err(|_| {
            let _ = child.kill();
            format!(
                "provider {} did not answer the handshake",
                program.display()
            )
        })?;
        if info.hello != PROTOCOL {
            let _ = child.kill();
            return Err(format!(
                "provider {} speaks protocol {}, sg speaks {PROTOCOL}",
                program.display(),
                info.hello
            ));
        }
        Ok(Arc::new(Plugin {
            child: Mutex::new(child),
            writer: Mutex::new(writer),
            pending,
            next_id: AtomicU64::new(1),
            info,
            usage: Usage::default(),
            effort: std::env::var("SG_REASONING_EFFORT")
                .ok()
                .filter(|e| matches!(e.as_str(), "low" | "medium" | "high")),
        }))
    }

    /// A generator for one worker. Workers share the provider process.
    pub fn generator(self: &Arc<Self>, slot: Option<usize>) -> PluginGenerator {
        PluginGenerator {
            plugin: self.clone(),
            slot,
        }
    }

    fn ask(&self, mut request: Request) -> Result<Reply, String> {
        request.id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = channel();
        self.pending
            .lock()
            .map_err(|_| "provider state poisoned".to_string())?
            .insert(request.id, tx);
        let line = serde_json::to_string(&request).map_err(|e| e.to_string())?;
        {
            let mut w = self
                .writer
                .lock()
                .map_err(|_| "provider writer poisoned".to_string())?;
            if let Err(e) = writeln!(w, "{line}").and_then(|_| w.flush()) {
                if let Ok(mut p) = self.pending.lock() {
                    p.remove(&request.id);
                }
                return Err(format!("could not send to the provider: {e}"));
            }
        }
        match rx.recv_timeout(REPLY_TIMEOUT) {
            Ok(Ok(reply)) => match reply.error.clone() {
                Some(e) => Err(e),
                None => Ok(reply),
            },
            Ok(Err(e)) => Err(e),
            Err(_) => {
                if let Ok(mut p) = self.pending.lock() {
                    p.remove(&request.id);
                }
                Err("the provider did not answer in time".to_string())
            }
        }
    }
}

impl Drop for Plugin {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub struct PluginGenerator {
    plugin: Arc<Plugin>,
    slot: Option<usize>,
}

pub const SYSTEM: &str =
    "You are a precise Rust engineer. Output only what is asked, no commentary.";
pub const MAX_TOKENS: u32 = 16000;

impl Generator for PluginGenerator {
    fn generate(&mut self, prompt: &str) -> Result<String, String> {
        let started = Instant::now();
        let reply = self.plugin.ask(Request {
            prompt: prompt.to_string(),
            system: Some(SYSTEM.to_string()),
            slot: self.slot,
            max_tokens: Some(MAX_TOKENS),
            effort: self.plugin.effort.clone(),
            ..Request::default()
        })?;
        let usage = &self.plugin.usage;
        usage.input.fetch_add(reply.input_tokens, Ordering::Relaxed);
        usage
            .output
            .fetch_add(reply.output_tokens, Ordering::Relaxed);
        let n = usage.calls.fetch_add(1, Ordering::Relaxed) + 1;
        eprintln!(
            "  model call {n}: {}s, {}+{} tokens",
            started.elapsed().as_secs(),
            reply.input_tokens,
            reply.output_tokens
        );
        reply
            .text
            .ok_or_else(|| "the provider sent no text".to_string())
    }
}

/// Where to find the provider program called `name`: a path as given, otherwise
/// `sg-provider-<name>` beside the running program and then on `PATH`.
pub fn find_provider(name: &str) -> Result<PathBuf, String> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return if p.exists() {
            Ok(p)
        } else {
            Err(format!("no provider program at {name}"))
        };
    }
    let file = format!("{PROVIDER_PREFIX}{name}");
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        let beside = dir.join(&file);
        if beside.exists() {
            return Ok(beside);
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&paths) {
            let candidate = dir.join(&file);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!(
        "no provider named \"{name}\": looked for {file} beside sg and on PATH. \
         Providers are separate programs; see the plugin module for the protocol."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// When this test binary is started as a provider (by the tests below) this runs the
    /// provider loop; in a normal test run it does nothing.
    #[test]
    fn provider_child() {
        let Ok(mode) = std::env::var("SG_TEST_PROVIDER") else {
            return;
        };
        println!();
        serve("test-provider", 5, move |r| match mode.as_str() {
            "fail" => Err(format!("refused: {}", r.prompt)),
            "slow-first" => {
                if r.prompt == "first" {
                    std::thread::sleep(Duration::from_millis(300));
                }
                Ok(Completion {
                    text: format!("echo {}", r.prompt),
                    input_tokens: 10,
                    output_tokens: 2,
                })
            }
            "die" => std::process::exit(3),
            _ => Ok(Completion {
                text: format!("{}|{:?}|{:?}", r.prompt, r.slot, r.effort),
                input_tokens: 10,
                output_tokens: 2,
            }),
        })
        .unwrap();
    }

    fn start(mode: &str) -> Arc<Plugin> {
        let exe = std::env::current_exe().unwrap();
        // The harness prints its own lines to stdout; the protocol ignores lines that are not
        // JSON for it, so they do not matter. The test binary runs as the provider.
        let script =
            std::env::temp_dir().join(format!("sg-test-provider-{mode}-{}.sh", std::process::id()));
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nSG_TEST_PROVIDER={mode} exec '{}' --exact plugin::tests::provider_child --nocapture --test-threads 1\n",
                exe.display()
            ),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        Plugin::spawn(&script, &[]).unwrap()
    }

    #[test]
    fn the_handshake_reports_the_name_and_channels() {
        let p = start("echo");
        assert_eq!(p.info.name, "test-provider");
        assert_eq!(p.info.channels, 5);
    }

    #[test]
    fn a_request_carries_its_slot_and_the_reply_its_token_counts() {
        let p = start("echo");
        let mut g = p.generator(Some(3));
        let text = g.generate("hello").unwrap();
        assert_eq!(text, "hello|Some(3)|None");
        assert_eq!(p.usage.input.load(Ordering::Relaxed), 10);
        assert_eq!(p.usage.output.load(Ordering::Relaxed), 2);
        assert_eq!(p.usage.calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn many_workers_share_one_provider_and_each_gets_its_own_answer() {
        let p = start("slow-first");
        let answers: Vec<(String, String)> = std::thread::scope(|s| {
            let handles: Vec<_> = ["first", "second", "third", "fourth"]
                .iter()
                .enumerate()
                .map(|(i, prompt)| {
                    let mut g = p.generator(Some(i));
                    s.spawn(move || ((*prompt).to_string(), g.generate(prompt).unwrap()))
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        for (prompt, answer) in answers {
            assert_eq!(answer, format!("echo {prompt}"));
        }
        assert_eq!(p.usage.calls.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn an_error_reply_is_an_error_for_that_request_only() {
        let p = start("fail");
        let err = p.generator(None).generate("nope").unwrap_err();
        assert_eq!(err, "refused: nope");
    }

    #[test]
    fn a_provider_that_exits_fails_the_waiting_request_instead_of_hanging() {
        let p = start("die");
        let err = p.generator(None).generate("anything").unwrap_err();
        assert!(err.contains("exited"), "{err}");
    }

    #[test]
    fn a_program_that_never_greets_is_not_accepted_as_a_provider() {
        let script = std::env::temp_dir().join(format!("sg-test-mute-{}.sh", std::process::id()));
        std::fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let err = Plugin::spawn(&script, &[]).err().expect("must fail");
        assert!(err.contains("handshake"), "{err}");
    }

    #[test]
    fn serve_answers_the_handshake_and_requests_in_memory() {
        let input = b"{\"hello\":1}\n{\"id\":4,\"prompt\":\"x\"}\nnot json\n";
        let out = Arc::new(Mutex::new(Vec::<u8>::new()));
        serve_on(&input[..], out.clone(), "mem", 2, |r| {
            Ok(Completion {
                text: r.prompt.to_uppercase(),
                ..Completion::default()
            })
        })
        .unwrap();
        let text = String::from_utf8(out.lock().unwrap().clone()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        let hello: Hello = serde_json::from_str(lines[0]).unwrap();
        assert_eq!((hello.name.as_str(), hello.channels), ("mem", 2));
        let reply: Reply = serde_json::from_str(lines[1]).unwrap();
        assert_eq!((reply.id, reply.text.as_deref()), (4, Some("X")));
    }

    #[test]
    fn a_named_provider_is_looked_up_by_its_prefix_and_a_path_is_taken_as_given() {
        let err = find_provider("definitely-not-installed").unwrap_err();
        assert!(
            err.contains("sg-provider-definitely-not-installed"),
            "{err}"
        );
        assert!(find_provider("/no/such/provider").is_err());
        let exe = std::env::current_exe().unwrap();
        assert_eq!(find_provider(exe.to_str().unwrap()).unwrap(), exe);
    }
}
