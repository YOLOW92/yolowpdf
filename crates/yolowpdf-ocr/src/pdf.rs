//! 把 [`Engine`] 接到 PDF 的扫描页接口上：第一张扫描图来时才准备模型（缺的先下载，由调用方提供
//! 下载函数，本 crate 不碰网络）、才载入，没遇到扫描页的文档一点开销都没有。
//!
//! 准备模型不能堵住 CPU 池（它还负责正文提取和缓存编解码）：模型齐全时由第一个调用者载入，其他调用者等
//! 几秒；缺模型时常驻服务在独立线程里下载，这期间识别立刻返回错误，文档照常返回并带警告。
//! 失败不永久缓存，退避一段时间后再试。

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use yolowpdf::{OcrInfo, OcrLine, OcrOutput, PageOcr};

use crate::device::{Device, ModelChoice, ModelSuite, Quality, RecScript, select_models_with};
use crate::math::Math;
use crate::models::Model;
use crate::pipeline::Engine;
use crate::{Image, Line};

/// 把模型下载到 `<缓存根>/<仓库名>`（只在文件不全时调用），边下边校验。
pub type Fetch = Arc<dyn Fn(&Model) -> Result<(), String> + Send + Sync>;

/// 模型和设备怎么定。
pub struct PdfOcrConfig {
    pub root: PathBuf,
    /// 模型套件：默认 [`ModelSuite::Yolow`]（MIT BF16），也可选 [`ModelSuite::Paddle`]。
    pub suite: ModelSuite,
    pub quality: Quality,
    /// 文字识别词表范围：默认 [`RecScript::Multi`]，纯英文/拉丁文档可选 [`RecScript::Latin`]。
    pub script: RecScript,
    pub device: Device,
    /// 调用方明确要了 GPU（`cuda`、`cuda:N`）。`auto` 时本机没有 N 卡不算退回，不报 `gpu_fallback`。
    pub gpu_required: bool,
    /// 也识别行间公式（自研轻量套约 94 MB，Paddle 上游约 860 MB）。
    pub formulas: bool,
    /// 没有时缺模型就报错（离线环境，提前把模型放进 `root`）。
    pub fetch: Option<Fetch>,
    /// 缺模型时放到独立线程下载，这期间识别立刻返回错误（常驻服务）；否则调用者等下载完（命令行、绑定）。
    pub background_download: bool,
}

impl PdfOcrConfig {
    /// 以默认策略构造配置：自研 MIT 套件、`Quality::Auto`、多语种、`Device::Cpu`、不启用公式。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            suite: ModelSuite::default(),
            quality: Quality::default(),
            script: RecScript::default(),
            device: Device::Cpu,
            gpu_required: false,
            formulas: false,
            fetch: None,
            background_download: false,
        }
    }

    pub fn suite(mut self, suite: ModelSuite) -> Self {
        self.suite = suite;
        self
    }

    pub fn quality(mut self, quality: Quality) -> Self {
        self.quality = quality;
        self
    }

    pub fn script(mut self, script: RecScript) -> Self {
        self.script = script;
        self
    }

    pub fn device(mut self, device: Device) -> Self {
        self.device = device;
        self
    }

    pub fn gpu_required(mut self, required: bool) -> Self {
        self.gpu_required = required;
        self
    }

    pub fn formulas(mut self, formulas: bool) -> Self {
        self.formulas = formulas;
        self
    }

    pub fn fetch(mut self, fetch: Fetch) -> Self {
        self.fetch = Some(fetch);
        self
    }

    pub fn background_download(mut self, background: bool) -> Self {
        self.background_download = background;
        self
    }

    /// 按当前配置选出的整套模型清单。
    pub fn selected_models(&self) -> ModelChoice {
        select_models_with(self.suite, self.quality, self.script, self.device)
    }
}

/// [策略] 下载或载入失败后隔这么久再试：网络抖动不该让 OCR 到重启前都不可用，也不该每张图都重试一次
const RETRY_AFTER: Duration = Duration::from_secs(60);

/// [`PdfOcr`] 准备模型的进度（模型载入成功后就不再看它）。
enum Attempt {
    /// 没有人在准备；上次失败退避完也回到这里。
    Idle,
    /// 有调用者在载入（或在前台下载），其他调用者等。
    Loading,
    /// 独立线程在下载，调用者不等。
    Downloading,
    Failed {
        error: String,
        at: Instant,
    },
}

/// 准备进度和通知，也交给下载线程。
struct Gate {
    attempt: Mutex<Attempt>,
    changed: Condvar,
}

impl Gate {
    fn lock(&self) -> MutexGuard<'_, Attempt> {
        self.attempt.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn finish(&self, next: Attempt) {
        *self.lock() = next;
        self.changed.notify_all();
    }
}

fn failed(error: String) -> Attempt {
    Attempt::Failed {
        error,
        at: Instant::now(),
    }
}

struct Loaded {
    engine: Engine,
    engine_name: String,
    model_name: String,
}

/// 懒加载的 PDF OCR 引擎。
pub struct PdfOcr {
    config: PdfOcrConfig,
    ready: OnceLock<Loaded>,
    gate: Arc<Gate>,
    retry_after: Duration,
}

impl PdfOcr {
    pub fn new(config: PdfOcrConfig) -> Self {
        Self {
            config,
            ready: OnceLock::new(),
            gate: Arc::new(Gate {
                attempt: Mutex::new(Attempt::Idle),
                changed: Condvar::new(),
            }),
            retry_after: RETRY_AFTER,
        }
    }

    /// 已载入的引擎（设备状态、GPU 统计从这里取）；还没遇到扫描页、没载入时为 None。
    pub fn engine(&self) -> Option<&Engine> {
        self.ready.get().map(|l| &l.engine)
    }

    /// 模型目录直接给（测评或自定义权重用）：立刻载入，不走缓存和下载。
    pub fn from_dirs(
        det_dir: PathBuf,
        rec_dir: PathBuf,
        math_dirs: Option<(PathBuf, PathBuf)>,
        device: Device,
    ) -> Result<Self, String> {
        let model_name = dir_model_label(&rec_dir);
        let engine_name = if model_name.to_ascii_lowercase().contains("pp-ocr") {
            ModelSuite::Paddle.engine_name()
        } else {
            ModelSuite::Yolow.engine_name()
        };
        let this = Self::new(
            PdfOcrConfig::new(PathBuf::new())
                .device(device)
                .formulas(math_dirs.is_some()),
        );
        let mut engine = Engine::load_with(&det_dir, &rec_dir, device)?;
        if let Some((layout, formula)) = math_dirs {
            engine = engine.with_math(Math::load(&layout, &formula)?);
        }
        let _ = this.ready.set(Loaded {
            engine,
            engine_name: engine_name.to_owned(),
            model_name,
        });
        Ok(this)
    }

    /// 载入好的模型。别人在载入时等（最多等到 `deadline`）；还在下载、失败退避中时立刻返回原因。
    fn loaded(&self, deadline: Option<Instant>) -> Result<&Loaded, String> {
        if let Some(l) = self.ready.get() {
            return Ok(l);
        }
        let mut attempt = self.gate.lock();
        loop {
            if let Some(l) = self.ready.get() {
                return Ok(l);
            }
            match &*attempt {
                Attempt::Idle => break,
                Attempt::Downloading => {
                    return Err("OCR models are still downloading".into());
                }
                Attempt::Failed { error, at } if at.elapsed() < self.retry_after => {
                    return Err(error.clone());
                }
                Attempt::Failed { .. } => break,
                Attempt::Loading => {}
            }
            attempt = match deadline {
                None => self
                    .gate
                    .changed
                    .wait(attempt)
                    .unwrap_or_else(|e| e.into_inner()),
                Some(d) => {
                    let wait = d.saturating_duration_since(Instant::now());
                    if wait.is_zero() {
                        return Err("the OCR deadline passed while models were loading".into());
                    }
                    self.gate
                        .changed
                        .wait_timeout(attempt, wait)
                        .unwrap_or_else(|e| e.into_inner())
                        .0
                }
            };
        }
        *attempt = Attempt::Loading;
        drop(attempt);
        self.load_or_download()
    }

    /// 轮到这个调用者准备模型。模型文件上的问题（panic 也算）只记成失败：别的调用者还在等结果。
    fn load_or_download(&self) -> Result<&Loaded, String> {
        let c = &self.config;
        let result = catch_unwind(AssertUnwindSafe(|| {
            let choice = c.selected_models();
            let missing: Vec<Model> = choice
                .models(c.formulas)
                .into_iter()
                .filter(|m| !m.present(&c.root))
                .collect();
            if !missing.is_empty() {
                let Some(fetch) = &c.fetch else {
                    return Err(format!(
                        "model {} is missing from {}; run `yolowpdf models fetch`",
                        missing[0].repo,
                        c.root.display()
                    ));
                };
                if c.background_download {
                    return self
                        .download_in_background(fetch.clone(), missing)
                        .map(|()| None);
                }
                missing.iter().try_for_each(|m| fetch(m))?;
            }
            self.load(choice).map(Some)
        }))
        .unwrap_or_else(|_| Err("loading the OCR models panicked".into()));
        match result {
            Ok(Some(loaded)) => {
                let _ = self.ready.set(loaded);
                self.gate.finish(Attempt::Idle);
                self.ready
                    .get()
                    .ok_or_else(|| "the OCR models are not loaded".to_owned())
            }
            // 下载线程已经接手，进度由它维护
            Ok(None) => Err("OCR models are still downloading".into()),
            Err(error) => {
                self.gate.finish(failed(error.clone()));
                Err(error)
            }
        }
    }

    /// 在独立线程里下载缺的模型。下载完回到空闲，下一个调用者来载入。
    fn download_in_background(&self, fetch: Fetch, missing: Vec<Model>) -> Result<(), String> {
        // 先改状态再起线程：线程可能很快结束
        *self.gate.lock() = Attempt::Downloading;
        let gate = self.gate.clone();
        std::thread::Builder::new()
            .name("yolowpdf-ocr-download".into())
            .spawn(move || {
                let done = catch_unwind(AssertUnwindSafe(|| {
                    missing.iter().try_for_each(|m| fetch(m))
                }))
                .unwrap_or_else(|_| Err("downloading the OCR models panicked".into()));
                gate.finish(match done {
                    Ok(()) => Attempt::Idle,
                    Err(error) => failed(error),
                });
            })
            .map(|_| ())
            .map_err(|e| format!("failed to start the model download thread: {e}"))
    }

    fn load(&self, choice: ModelChoice) -> Result<Loaded, String> {
        let c = &self.config;
        let mut engine =
            Engine::load_with(&choice.det.dir(&c.root), &choice.rec.dir(&c.root), c.device)?;
        if c.formulas {
            engine = engine.with_math(Math::load(
                &choice.layout.dir(&c.root),
                &choice.formula.dir(&c.root),
            )?);
        }
        Ok(Loaded {
            engine,
            engine_name: choice.engine_name().to_owned(),
            model_name: choice.rec.repo.to_owned(),
        })
    }
}

impl std::fmt::Debug for PdfOcr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfOcr")
            .field("suite", &self.config.suite)
            .field("quality", &self.config.quality)
            .field("script", &self.config.script)
            .field("device", &self.config.device)
            .finish_non_exhaustive()
    }
}

/// 从本地权重目录推导可读模型名（遇到 `…/<model>/best` 时取上一级目录名）。
fn dir_model_label(dir: &std::path::Path) -> String {
    let leaf = dir.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if (leaf == "best" || leaf == "best_bf16")
        && let Some(parent) = dir
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
    {
        return parent.to_owned();
    }
    if leaf.is_empty() {
        dir.display().to_string()
    } else {
        leaf.to_owned()
    }
}

/// 外框（各角 x、y 的最小、最大）。
fn to_ocr_line(l: Line) -> OcrLine {
    let xs = l.quad.map(|p| p.0);
    let ys = l.quad.map(|p| p.1);
    let min = |v: [f32; 4]| v.into_iter().fold(f32::MAX, f32::min);
    let max = |v: [f32; 4]| v.into_iter().fold(f32::MIN, f32::max);
    OcrLine {
        x0: min(xs),
        y0: min(ys),
        x1: max(xs),
        y1: max(ys),
        formula: l.formula,
        text: l.text,
        score: l.score,
    }
}

impl PageOcr for PdfOcr {
    fn prepare(&self, deadline: Option<Instant>) -> Result<(), String> {
        self.loaded(deadline).map(|_| ())
    }

    fn recognize(
        &self,
        width: u32,
        height: u32,
        gray: &[u8],
        deadline: Option<Instant>,
    ) -> Result<OcrOutput, String> {
        let loaded = self.loaded(deadline)?;
        let img = Image {
            width: width as usize,
            height: height as usize,
            channels: 1,
            data: gray.to_vec(),
        };
        let run = loaded.engine.run_until(&img, deadline)?;
        // auto 模式在没有 N 卡的机器上用 CPU 是正常情况，不算退回
        let expected = self.config.gpu_required || yolowpdf_gpu::device_count() > 0;
        Ok(OcrOutput {
            lines: run.lines.into_iter().map(to_ocr_line).collect(),
            device: Some(device_name(run.device)),
            fallback: run.fallback.filter(|_| expected),
            partial_error: run.partial_error,
        })
    }

    fn info(&self) -> OcrInfo {
        let planned = self.config.selected_models();
        let mut info = OcrInfo {
            engine: planned.engine_name().into(),
            model: planned.rec.repo.into(),
            device: "cpu".into(),
            ..Default::default()
        };
        match self.ready.get() {
            None => {
                if let Attempt::Failed { error, .. } = &*self.gate.lock() {
                    info.error = Some(error.clone());
                }
            }
            Some(l) => {
                info.engine = l.engine_name.clone();
                info.model = l.model_name.clone();
                let status = l.engine.device_status();
                info.device = device_name(status.actual);
                // auto 模式在没有 N 卡的机器上用 CPU 是正常情况，不算退回
                let expected = self.config.gpu_required || yolowpdf_gpu::device_count() > 0;
                if status.fell_back() && expected {
                    info.fallback = status.fallback;
                }
            }
        }
        info
    }
}

/// `cpu` 或 `cuda:N`。
fn device_name(d: Device) -> String {
    match d {
        Device::Cpu => "cpu".into(),
        Device::Gpu { index } => format!("cuda:{index}"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{Sender, channel};

    use super::*;

    /// 一个不存在的模型目录：模型都算缺的。
    fn config(fetch: Option<Fetch>, background_download: bool) -> PdfOcrConfig {
        let mut c = PdfOcrConfig::new("/nonexistent/yolowpdf-ocr-test")
            .quality(Quality::Fast)
            .device(Device::Cpu)
            .background_download(background_download);
        if let Some(f) = fetch {
            c = c.fetch(f);
        }
        c
    }

    fn counting(calls: &Arc<AtomicUsize>) -> Fetch {
        let calls = calls.clone();
        Arc::new(move |_: &Model| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err("network down".to_owned())
        })
    }

    /// 下载失败不永久缓存：退避期内直接报原因，退避过后再试。
    #[test]
    fn a_failed_download_is_retried_after_the_backoff() {
        let calls = Arc::new(AtomicUsize::new(0));
        let mut ocr = PdfOcr::new(config(Some(counting(&calls)), false));
        assert_eq!(ocr.prepare(None), Err("network down".to_owned()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        // 退避期内不再下载，info 也如实报告
        assert_eq!(ocr.prepare(None), Err("network down".to_owned()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(ocr.info().error.as_deref(), Some("network down"));
        // 退避过后再试
        ocr.retry_after = Duration::ZERO;
        assert_eq!(ocr.prepare(None), Err("network down".to_owned()));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    /// 没有下载函数时报缺模型；之后补上文件也能恢复（不永久缓存），这里只看错误信息。
    #[test]
    fn missing_models_without_a_fetcher_say_what_to_do() {
        let ocr = PdfOcr::new(config(None, true));
        let e = ocr.prepare(None).unwrap_err();
        assert!(e.contains("yolowpdf models fetch"), "{e}");
    }

    fn wait_for(ocr: &PdfOcr, f: impl Fn(&Attempt) -> bool) {
        // wall-clock: 轮询的上界，慢机器只是多等
        let end = Instant::now() + Duration::from_secs(30);
        let mut a = ocr.gate.lock();
        while !f(&a) {
            assert!(Instant::now() < end, "等状态超时");
            a = ocr
                .gate
                .changed
                .wait_timeout(a, Duration::from_millis(50))
                .unwrap()
                .0;
        }
    }

    /// 常驻服务：下载放到独立线程，调用者立刻拿到“还在下载”；下载失败后退避、再试。
    #[test]
    fn background_download_never_blocks_the_caller() {
        let (go_tx, go_rx) = channel::<()>();
        let (seen_tx, seen_rx) = channel::<()>();
        let go = Mutex::new(go_rx);
        let seen: Mutex<Sender<()>> = Mutex::new(seen_tx);
        let fetch: Fetch = Arc::new(move |_: &Model| {
            seen.lock().unwrap().send(()).unwrap();
            // 卡住，直到测试放行
            go.lock().unwrap().recv().unwrap();
            Err("network down".to_owned())
        });
        let mut ocr = PdfOcr::new(config(Some(fetch), true));
        let e = ocr.prepare(None).unwrap_err();
        assert!(e.contains("still downloading"), "{e}");
        seen_rx.recv_timeout(Duration::from_secs(30)).unwrap();
        // 下载线程还卡着，调用者（包括 recognize）照样立刻返回
        // wall-clock: 下载线程在等 go_tx，等它的 bug 会一直卡住；10 s 远大于正常的几微秒
        let t = Instant::now();
        assert!(ocr.prepare(None).unwrap_err().contains("still downloading"));
        let e = ocr.recognize(1, 1, &[255], None).unwrap_err();
        assert!(e.contains("still downloading"), "{e}");
        assert!(t.elapsed() < Duration::from_secs(10));
        go_tx.send(()).unwrap();
        wait_for(&ocr, |a| matches!(a, Attempt::Failed { .. }));
        assert_eq!(ocr.prepare(None), Err("network down".to_owned()));
        // 退避过后重新起一个下载线程
        ocr.retry_after = Duration::ZERO;
        assert!(ocr.prepare(None).unwrap_err().contains("still downloading"));
        seen_rx.recv_timeout(Duration::from_secs(30)).unwrap();
        go_tx.send(()).unwrap();
        wait_for(&ocr, |a| matches!(a, Attempt::Failed { .. }));
    }

    /// 有人在载入时其他调用者等，等到截止时间就报错，不永远占着工人。
    #[test]
    fn waiters_give_up_at_their_deadline() {
        let ocr = PdfOcr::new(config(None, false));
        *ocr.gate.lock() = Attempt::Loading;
        let e = ocr
            // wall-clock: 截止时间要等它过去才报错，机器慢只会过得更早
            .prepare(Some(Instant::now() + Duration::from_millis(20)))
            .unwrap_err();
        assert!(e.contains("deadline"), "{e}");
    }

    #[test]
    fn builder_defaults_to_yolow_and_supports_suite_and_script() {
        let cfg = PdfOcrConfig::new("/tmp/models")
            .script(RecScript::Latin)
            .formulas(true);
        let choice = cfg.selected_models();
        assert_eq!(choice.suite, ModelSuite::Yolow);
        assert_eq!(choice.script, RecScript::Latin);
        assert_eq!(choice.models(true).len(), 4);
        assert_eq!(choice.engine_name(), "yolowocr");

        let ocr = PdfOcr::new(cfg);
        let info = ocr.info();
        assert_eq!(info.engine, "yolowocr");
        assert!(info.model.contains("rec_v2_latin"), "{}", info.model);

        assert_eq!(
            dir_model_label(std::path::Path::new("/exports/v3_bf16/rec_v3_large/best")),
            "rec_v3_large"
        );
        assert_eq!(
            dir_model_label(std::path::Path::new(
                "/cache/PP-OCRv6_small_rec_safetensors"
            )),
            "PP-OCRv6_small_rec_safetensors"
        );
    }
}
