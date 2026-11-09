//! Benchmark worker: one PDF per stdin line, one JSON reply per line.
//! `{"id":"...","input":"/abs/file.pdf","out":"/abs/file.md"}`
use std::io::{BufRead, Write};
use std::path::Path;
use std::time::Instant;

use serde_json::{Value, json};

fn reply(v: Value) {
    println!("{v}");
    std::io::stdout().flush().ok();
}

fn main() {
    reply(json!({"ready": true, "tool": "yolowpdf", "version": env!("CARGO_PKG_VERSION")}));
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let job: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                reply(json!({"id": null, "ok": false, "error": format!("bad job: {e}")}));
                continue;
            }
        };
        let id = job.get("id").cloned().unwrap_or(Value::Null);
        let input = job["input"].as_str().unwrap_or_default();
        let out = job["out"].as_str().unwrap_or_default();
        let t = Instant::now();
        let result = std::fs::read(input)
            .map_err(|e| e.to_string())
            .and_then(|bytes| yolowpdf::extract(&bytes).map_err(|e| e.to_string()));
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        match result {
            Ok(doc) => {
                let text = doc.markdown;
                let bytes = text.len();
                if let Some(parent) = Path::new(out).parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                if let Err(e) = std::fs::write(out, &text) {
                    reply(
                        json!({"id": id, "ok": false, "ms": ms, "bytes": 0, "error": e.to_string()}),
                    );
                } else {
                    reply(
                        json!({"id": id, "ok": true, "ms": ms, "bytes": bytes, "error": null,
                                 "pages": doc.pages.len(), "scanned": doc.scanned,
                                 "ocr": doc.ocr.ran, "device": doc.ocr.device}),
                    );
                }
            }
            Err(e) => reply(json!({"id": id, "ok": false, "ms": ms, "bytes": 0, "error": e})),
        }
    }
}
