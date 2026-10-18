use crate::assemble::Assembled;
use crate::model::NodeId;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug, Default)]
pub struct Report {
    pub ok: bool,
    /// Compiler messages attributed to the node whose span contains them.
    pub by_node: BTreeMap<NodeId, Vec<String>>,
    /// Messages that fall outside every node span.
    pub unattributed: Vec<String>,
}

/// Write `assembled.files` under `crate_dir` (an existing cargo crate), run
/// `cargo check --message-format=json`, and attribute each error to a node via
/// `assembled.spans`. Warnings are ignored.
pub fn check(crate_dir: &Path, assembled: &Assembled) -> std::io::Result<Report> {
    for (path, text) in &assembled.files {
        let full = crate_dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(full, text)?;
    }

    let output = Command::new("cargo")
        .args(["check", "--message-format=json"])
        .current_dir(crate_dir)
        .env_remove("CARGO_TARGET_DIR")
        .stdin(Stdio::null())
        .output()?;

    let mut report = Report {
        ok: output.status.success(),
        ..Report::default()
    };

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value["reason"] != "compiler-message" {
            continue;
        }
        let message = &value["message"];
        if message["level"] != "error" {
            continue;
        }
        let text = message["rendered"]
            .as_str()
            .or_else(|| message["message"].as_str())
            .unwrap_or_default()
            .to_string();
        match primary_location(message) {
            Some((file, line)) => match find_node(assembled, Path::new(&file), line) {
                Some(id) => report.by_node.entry(id).or_default().push(text),
                None => report.unattributed.push(text),
            },
            None => report.unattributed.push(text),
        }
    }

    if !report.ok && report.by_node.is_empty() && report.unattributed.is_empty() {
        report
            .unattributed
            .push(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(report)
}

fn primary_location(message: &Value) -> Option<(String, usize)> {
    let span = message["spans"]
        .as_array()?
        .iter()
        .find(|s| s["is_primary"] == true)?;
    Some((
        span["file_name"].as_str()?.to_string(),
        span["line_start"].as_u64()? as usize,
    ))
}

fn find_node(assembled: &Assembled, file: &Path, line: usize) -> Option<NodeId> {
    assembled
        .spans
        .iter()
        .find(|(_, s)| s.file == file && s.start_line <= line && line <= s.end_line)
        .map(|(id, _)| id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assemble::Span;
    use std::path::PathBuf;

    fn temp_crate(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sg_verify_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"tiny\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[workspace]\n",
        )
        .unwrap();
        dir
    }

    fn assembled(src: &str, spans: &[(&str, usize, usize)]) -> Assembled {
        let file = PathBuf::from("src/lib.rs");
        Assembled {
            files: BTreeMap::from([(file.clone(), src.to_string())]),
            spans: spans
                .iter()
                .map(|(id, a, b)| {
                    (
                        NodeId(id.to_string()),
                        Span {
                            file: file.clone(),
                            start_line: *a,
                            end_line: *b,
                        },
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn passing_crate_is_ok() {
        let dir = temp_crate("pass");
        let a = assembled("pub fn good() -> u32 { 1 }\n", &[("good", 1, 1)]);
        let report = check(&dir, &a).unwrap();
        assert!(report.ok);
        assert!(report.by_node.is_empty() && report.unattributed.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failing_node_is_attributed() {
        let dir = temp_crate("fail");
        let src = "pub fn good() -> u32 { 1 }\npub fn bad() -> u32 { \"x\" }\n";
        let a = assembled(src, &[("good", 1, 1), ("bad", 2, 2)]);
        let report = check(&dir, &a).unwrap();
        assert!(!report.ok);
        assert_eq!(report.by_node.len(), 1);
        assert!(report.by_node.contains_key(&NodeId("bad".into())));
        assert!(report.unattributed.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn error_outside_spans_is_unattributed() {
        let dir = temp_crate("unattr");
        let src = "pub fn bad() -> u32 { \"x\" }\n";
        let a = assembled(src, &[]);
        let report = check(&dir, &a).unwrap();
        assert!(!report.ok);
        assert_eq!(report.unattributed.len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
