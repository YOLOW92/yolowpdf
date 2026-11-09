//! OCR benchmark worker: one PDF per stdin line, one JSON reply per line.
//! Model directories come from env:
//!   YOLOWPDF_OCR_DET, YOLOWPDF_OCR_REC, optional YOLOWPDF_OCR_LAYOUT/YOLOWPDF_OCR_FORMULA
//!   OCR_DEVICE=cpu|gpu, PDF_OCR_FORCE=1 to OCR every page with images.
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde_json::{Value, json};
use yolowpdf_ocr::device::Device;
use yolowpdf_ocr::pdf::PdfOcr;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn reply(v: Value) {
    println!("{v}");
    std::io::stdout().flush().ok();
}

fn main() {
    let det = env_path("YOLOWPDF_OCR_DET").expect("YOLOWPDF_OCR_DET");
    let rec = env_path("YOLOWPDF_OCR_REC").expect("YOLOWPDF_OCR_REC");
    let math = match (
        env_path("YOLOWPDF_OCR_LAYOUT"),
        env_path("YOLOWPDF_OCR_FORMULA"),
    ) {
        (Some(l), Some(f)) => Some((l, f)),
        _ => None,
    };
    let device = match std::env::var("OCR_DEVICE").as_deref() {
        Ok("gpu") | Ok("cuda") => Device::Gpu { index: 0 },
        _ => Device::Cpu,
    };
    let engine = Arc::new(
        PdfOcr::from_dirs(det, rec, math, device).unwrap_or_else(|e| {
            eprintln!("loading OCR models: {e}");
            std::process::exit(2);
        }),
    );
    let opts = yolowpdf::Options {
        ocr: Some(engine.clone()),
        adaptive_timeout: false,
        force_ocr: std::env::var_os("PDF_OCR_FORCE").is_some(),
        ..Default::default()
    };
    reply(json!({"ready": true, "tool": "yolowpdf-ocr", "version": env!("CARGO_PKG_VERSION")}));
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
            .and_then(|bytes| yolowpdf::extract_with(&bytes, &opts).map_err(|e| e.to_string()));
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        match result {
            Ok(doc) => {
                let text = doc.markdown;
                let bytes = text.len();
                if let Some(parent) = std::path::Path::new(out).parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                if let Err(e) = std::fs::write(out, &text) {
                    reply(
                        json!({"id": id, "ok": false, "ms": ms, "bytes": 0, "error": e.to_string()}),
                    );
                } else {
                    let status = engine.engine().map(|e| {
                        let s = e.device_status();
                        json!({"actual": format!("{:?}", s.actual), "fallback": s.fallback})
                    });
                    reply(
                        json!({"id": id, "ok": true, "ms": ms, "bytes": bytes, "error": null,
                                 "pages": doc.pages.len(), "scanned": doc.scanned,
                                 "ocr": doc.ocr.ran, "device": doc.ocr.device,
                                 "ocr_error": doc.ocr.error, "engine": status}),
                    );
                }
            }
            Err(e) => reply(json!({"id": id, "ok": false, "ms": ms, "bytes": 0, "error": e})),
        }
    }
}
