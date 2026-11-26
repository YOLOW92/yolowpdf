//! 设备与模型的选择：跑在哪（[`Device`]）、实际跑在哪（[`DeviceStatus`]）、
//! 用哪套模型（[`ModelSuite`]）、哪档质量（[`Quality`]）、哪种字表（[`RecScript`]）。

use yolowpdf_gpu::GpuInfo;

use crate::models::{
    FORMULA, LAYOUT, MEDIUM_DET, MEDIUM_REC, Model, SMALL_DET, SMALL_REC, V2_DET, V2_FORMULA,
    V2_LAYOUT, V2_REC_LATIN, V2_REC_MULTI, V3_REC_LATIN, V3_REC_MULTI,
};

/// 想用的设备。配置 `[gpu] mode` 的对应：`off` → `Cpu`，`cuda` → `Gpu { index: 0 }`，
/// `cuda:N` → `Gpu { index: N }`；`auto` 也是 `Gpu { index: 0 }`，打不开时退回 CPU。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Cpu,
    Gpu { index: usize },
}

/// 实际用的设备，以及想用 GPU 却退回 CPU 的原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceStatus {
    /// 调用方要的设备。
    pub requested: Device,
    /// 实际用的设备。
    pub actual: Device,
    /// `actual` 是 GPU 时它的情况（显存空闲量是打开时的值）。
    pub gpu: Option<GpuInfo>,
    /// 永久退回 CPU 的原因；没有退回（包括本来就要 CPU）时为 None。个别页、行显存不够只算那一页的事，
    /// 不记在这里，见 [`PageRun::fallback`](crate::pipeline::PageRun::fallback)。
    pub fallback: Option<String>,
}

impl DeviceStatus {
    /// 是否想用 GPU 却退回了 CPU（对应警告 `gpu_fallback`）。
    pub fn fell_back(&self) -> bool {
        self.fallback.is_some()
    }

    /// 不想用 GPU。
    pub(crate) fn cpu() -> DeviceStatus {
        DeviceStatus {
            requested: Device::Cpu,
            actual: Device::Cpu,
            gpu: None,
            fallback: None,
        }
    }

    /// GPU 打不开，退回 CPU。
    pub(crate) fn fallback(index: usize, why: String) -> DeviceStatus {
        DeviceStatus {
            requested: Device::Gpu { index },
            actual: Device::Cpu,
            gpu: None,
            fallback: Some(why),
        }
    }

    /// GPU 打开了。
    pub(crate) fn on_gpu(index: usize, info: GpuInfo) -> DeviceStatus {
        let device = Device::Gpu { index };
        DeviceStatus {
            requested: device,
            actual: device,
            gpu: Some(info),
            fallback: None,
        }
    }
}

/// 模型套件：自研 MIT BF16 套件（默认）或上游 Apache-2.0 Paddle 套件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelSuite {
    /// 自研从零训练 BF16 套件（MIT 许可，含 MTP-4 无损自投机公式解码）。
    #[default]
    Yolow,
    /// 上游 PaddlePaddle 套件（Apache-2.0：PP-OCRv6 + PP-DocLayoutV3 + PP-FormulaNet_plus-L）。
    Paddle,
}

impl ModelSuite {
    /// 写入 [`yolowpdf::OcrInfo::engine`] 的引擎标识。
    pub const fn engine_name(self) -> &'static str {
        match self {
            Self::Yolow => "yolowocr",
            Self::Paddle => "pp-ocrv6",
        }
    }
}

/// 文字识别词表范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecScript {
    /// 多语种全字表（18,710 字：中/英/日/韩/欧文）。
    #[default]
    Multi,
    /// 精简拉丁/英文字表（254 字，仅自研套件支持；上游套件退回多语种）。
    Latin,
}

/// 配置 `[ocr] quality`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quality {
    /// 有 GPU 且显存放得下高精度档就选高精档，否则选轻量档。
    #[default]
    Auto,
    /// 轻量快速档。
    Fast,
    /// 高精度档，不看设备（CPU 上也用，只是慢）。
    Accurate,
}

/// 选中的整套 OCR 模型（检测、识别、版面、公式统一收口）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelChoice {
    pub det: Model,
    pub rec: Model,
    pub layout: Model,
    pub formula: Model,
    pub suite: ModelSuite,
    pub script: RecScript,
    /// 档位名：自研套件为 `"fast"` / `"accurate"`，上游套件为 `"small"` / `"medium"`。
    pub tier: &'static str,
}

impl ModelChoice {
    fn yolow(accurate: bool, script: RecScript) -> ModelChoice {
        let rec = match (accurate, script) {
            (false, RecScript::Multi) => V2_REC_MULTI,
            (false, RecScript::Latin) => V2_REC_LATIN,
            (true, RecScript::Multi) => V3_REC_MULTI,
            (true, RecScript::Latin) => V3_REC_LATIN,
        };
        ModelChoice {
            det: V2_DET,
            rec,
            layout: V2_LAYOUT,
            formula: V2_FORMULA,
            suite: ModelSuite::Yolow,
            script,
            tier: if accurate { "accurate" } else { "fast" },
        }
    }

    fn paddle_small(script: RecScript) -> ModelChoice {
        ModelChoice {
            det: SMALL_DET,
            rec: SMALL_REC,
            layout: LAYOUT,
            formula: FORMULA,
            suite: ModelSuite::Paddle,
            script,
            tier: "small",
        }
    }

    fn paddle_medium(script: RecScript) -> ModelChoice {
        ModelChoice {
            det: MEDIUM_DET,
            rec: MEDIUM_REC,
            layout: LAYOUT,
            formula: FORMULA,
            suite: ModelSuite::Paddle,
            script,
            tier: "medium",
        }
    }

    /// 实际需要准备的模型列表（供下载器与 [`crate::pdf::PdfOcr`] 共享同一真相源）。
    pub fn models(&self, formulas: bool) -> Vec<Model> {
        if formulas {
            vec![self.det, self.rec, self.layout, self.formula]
        } else {
            vec![self.det, self.rec]
        }
    }

    /// 引擎标识（`"yolowocr"` 或 `"pp-ocrv6"`）。
    pub const fn engine_name(&self) -> &'static str {
        self.suite.engine_name()
    }
}

/// 高精度档在显存里至少要占多少字节（权重加一页的工作内存）。
pub fn medium_vram_bytes() -> u64 {
    // [推导] B5 上实测的“真正需要”峰值 2288 MiB（benchmarks/history/1.0/g3，权重 + 在用张量，并行 1 页）。
    // 这是批大小按显存放开时的值，显存少时批会变小，所以这个门槛偏保守
    2288 << 20
}

/// 按质量档和设备选默认套件（自研 MIT [`ModelSuite::Yolow`]、多语种）的模型。
pub fn select_models(quality: Quality, device: Device) -> ModelChoice {
    select_models_with(ModelSuite::default(), quality, RecScript::default(), device)
}

/// 按套件、质量档、词表范围和设备选整套 OCR 模型。`Quality::Auto` 用 [`yolowpdf_gpu::probe`]
/// 查空闲显存；查不到（没有 GPU、`PAGESIFT_GPU=off`）时选轻量档。
pub fn select_models_with(
    suite: ModelSuite,
    quality: Quality,
    script: RecScript,
    device: Device,
) -> ModelChoice {
    let accurate = match (quality, device) {
        (Quality::Fast, _) | (Quality::Auto, Device::Cpu) => false,
        (Quality::Accurate, _) => true,
        (Quality::Auto, Device::Gpu { index }) => {
            yolowpdf_gpu::probe(index).is_ok_and(|info| info.memory_free >= medium_vram_bytes())
        }
    };
    match suite {
        ModelSuite::Yolow => ModelChoice::yolow(accurate, script),
        ModelSuite::Paddle => {
            if accurate {
                ModelChoice::paddle_medium(script)
            } else {
                ModelChoice::paddle_small(script)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 按质量档与套件选模型() {
        let g = Device::Gpu { index: 0 };
        let y_fast = select_models(Quality::Fast, g);
        assert_eq!(y_fast.suite, ModelSuite::Yolow);
        assert_eq!(y_fast.tier, "fast");
        assert_eq!(y_fast.det, V2_DET);
        assert_eq!(y_fast.rec, V2_REC_MULTI);
        assert_eq!(y_fast.layout, V2_LAYOUT);
        assert_eq!(y_fast.formula, V2_FORMULA);
        assert_eq!(y_fast.models(false).len(), 2);
        assert_eq!(y_fast.models(true).len(), 4);

        let y_latin = select_models_with(ModelSuite::Yolow, Quality::Fast, RecScript::Latin, g);
        assert_eq!(y_latin.rec, V2_REC_LATIN);

        assert_eq!(select_models(Quality::Auto, Device::Cpu).tier, "fast");
        let y_acc = select_models(Quality::Accurate, Device::Cpu);
        assert_eq!(y_acc.tier, "accurate");
        assert_eq!(y_acc.det, V2_DET);
        assert_eq!(y_acc.rec, V3_REC_MULTI);

        let y_acc_latin =
            select_models_with(ModelSuite::Yolow, Quality::Accurate, RecScript::Latin, g);
        assert_eq!(y_acc_latin.rec, V3_REC_LATIN);

        let p_fast = select_models_with(ModelSuite::Paddle, Quality::Fast, RecScript::Multi, g);
        assert_eq!(p_fast.tier, "small");
        assert_eq!(p_fast.det, SMALL_DET);
        assert_eq!(p_fast.layout, LAYOUT);
        assert_eq!(p_fast.formula, FORMULA);

        let p_acc = select_models_with(
            ModelSuite::Paddle,
            Quality::Accurate,
            RecScript::Multi,
            Device::Cpu,
        );
        assert_eq!(p_acc.tier, "medium");
        assert_eq!(p_acc.det, MEDIUM_DET);
    }

    #[test]
    fn 不存在的显卡退回_cpu_并带原因() {
        let e = yolowpdf_gpu::probe(9999).unwrap_err();
        let s = DeviceStatus::fallback(9999, e);
        assert_eq!(s.actual, Device::Cpu);
        assert!(s.fell_back());
        assert_eq!(s.requested, Device::Gpu { index: 9999 });
        assert_eq!(
            select_models(Quality::Auto, Device::Gpu { index: 9999 }).tier,
            "fast"
        );
        assert_eq!(
            select_models_with(
                ModelSuite::Paddle,
                Quality::Auto,
                RecScript::Multi,
                Device::Gpu { index: 9999 },
            )
            .tier,
            "small"
        );
    }
}
