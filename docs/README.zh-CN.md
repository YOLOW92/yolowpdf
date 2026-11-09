# yolowpdf

[English](../README.md)

用 Rust 写的 PDF → Markdown 库和 OCR 引擎，从
[PageSift](https://github.com/YOLOW92/pagesift) 提取而来。它自己读 PDF 文件格式：
不依赖 Poppler、MuPDF 或其他本地 PDF 库。

## 故事

PageSift 是一个抓取和正文抽取引擎。它的 PDF 分支一开始只有一个要求：把互联网上
拿到的 PDF 变成干净的 Markdown，不要让每台机器都装一套本地 PDF 工具链。

真实文件并不干净：坏的 xref、对象流、加密流、没有 ToUnicode 的 CID 字体、多栏
排版、表格、行内公式、图片、扫描页。处理这些问题最后长成了一个独立的 PDF 阅读器：
文件结构、字体、版面、表格、公式、图片，再留一个 OCR 接口。扫描页又需要 OCR，
于是有了第二个 crate。

这份代码从 PageSift 1.2.4（提交 `07a7053e`）提取出来，由版权持有人以 MIT
重新授权，成为独立的 `yolowpdf` 工作区。核心处理带文字层的文档；可选 OCR crate
让扫描页通过同一接口进入处理流程。解析快或输出非空，并不意味着困难页面一定被正确理解。

## 一眼看过

- `yolowpdf`：PDF 解析、文字提取、版面重建、表格、公式、图片、扫描页检测。不依赖
  任何模型。
- `yolowpdf-ocr`：PP-OCRv6 文字检测和识别，可选版面模型和公式模型，CPU 或
  NVIDIA GPU。
- API 很小：`extract`、`extract_with`，以及 `PageOcr` trait。
- 运行时依赖都是宽松许可证；PDF 核心没有本地库，也没有模型运行时。
- Rust 1.89 或更新；edition 2024。
- MIT 许可证。见 [LICENSE](../LICENSE)、[THIRD-PARTY.md](../THIRD-PARTY.md)、
  [UPSTREAM.md](../UPSTREAM.md)。

## 安装

还没发布到 crates.io，直接用 Git：

```toml
[dependencies]
yolowpdf = { git = "https://github.com/YOLOW92/yolowpdf" }

# 可选 OCR：
yolowpdf-ocr = { git = "https://github.com/YOLOW92/yolowpdf" }
```

## 用法

### PDF 转 Markdown

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = std::fs::read("document.pdf")?;
    let doc = yolowpdf::extract(&bytes)?;

    println!("{}", doc.markdown);
    println!("页数: {}", doc.pages.len());
    println!("扫描件: {}", doc.scanned);
    Ok(())
}
```

带限制和图片：

```rust
use std::time::Duration;

let opts = yolowpdf::Options {
    timeout: Some(Duration::from_secs(30)),
    images: yolowpdf::ImageOptions {
        markdown: true,
        ..Default::default()
    },
    ..Default::default()
};

let doc = yolowpdf::extract_with(&bytes, &opts)?;
for warning in &doc.warnings {
    eprintln!("warning: {warning}");
}
```

`PdfDocument` 给出整篇 Markdown、逐页 Markdown、元数据、页质量、图片引用、文档
警告和 OCR 报告。`Error` 有 `Malformed`、`Encrypted`、`TooLarge`、`Timeout`；
解析器 panic 会被接住并作为错误返回，不会带崩调用方。

### 扫描页 OCR

```rust
use std::sync::Arc;
use yolowpdf_ocr::device::Device;
use yolowpdf_ocr::pdf::PdfOcr;

let engine = Arc::new(PdfOcr::from_dirs(
    det_dir,          // PP-OCRv6 检测模型
    rec_dir,          // PP-OCRv6 识别模型
    None,             // 或 Some((layout_dir, formula_dir))
    Device::Cpu,      // 或 Device::Gpu { index: 0 }
)?);

let opts = yolowpdf::Options {
    ocr: Some(engine),
    ..Default::default()
};

let doc = yolowpdf::extract_with(&bytes, &opts)?;
```

`PdfOcr::from_dirs` 会立即加载指定模型；使用配置创建的 `PdfOcr::new` 则在首次
OCR 请求时加载。GPU 打不开时退回 CPU，并带回
原因。模型清单、固定版本和 SHA-256 在 `crates/yolowpdf-ocr/src/models.rs`；模型由
调用方下载，不随仓库分发。完整接口见 [docs/API.md](API.md)。

并发是文档级的：一次 `extract_with` 处理一份文档；要吞吐就多线程/多进程跑多份。
OCR 引擎内部有 GPU 批调度，多个文档 worker 可以喂同一张卡。

## 性能

以下是公开 olmOCR-bench 数据上的探索性测试，机器为 i7-10700、15 GB 内存、GTX 1070。
目标是 16 个 worker，但资源准入使部分工具的实际并发更低；公式模式使用 2 个。
各工具功能、启动和 I/O 口径不同。完成仅表示输出非空，包含占位文本的情况也可能计入，
不等于识别正确。峰值内存是单 worker 的 cgroup 峰值，不是进程总 RSS。

Docling 最后运行并受时间上限截断，未完成页面在评分时计失败，分数混合了质量和完成率。
部分评分使用了尚未验证等价性的公式预筛，**不能称为严格官方分**；共同完成页面的质量
对比也未完成。[详细报告](../benchmarks/results/2026-11-09/report.md)列出实际并发、
条件和原始结果。完成数、吞吐和分数越高越好，时间和内存越低越好。

### PDF：olmOCR-bench，1,269 页电子版

| 工具 | 完成 ↑ | wall ↓ | 页/s ↑ | p50 ↓ | p95 ↓ | 峰值内存 ↓ | 评分 ↑ |
|---|---:|---:|---:|---:|---:|---:|---:|
| **yolowpdf** | **1269/1269** | **0.668 s** | **1,900** | **3.2 ms** | **9.0 ms** | **24 MB** | **0.592** |
| pymupdf4llm | 1213/1269 | 184.933 s | 6.56 | 845.6 ms | 1,980.5 ms | 600 MB | 0.489 |
| pypdf | 1213/1269 | 15.285 s | 79.4 | 73.9 ms | 233.5 ms | 186 MB | 0.385 |
| MarkItDown | 1204/1269 | 88.469 s | 13.6 | 172.8 ms | 518.2 ms | 1,171 MB | 0.373 |
| Docling GPU | 589/1269 | 185.150 s | 3.18 | 364.2 ms | 1,823.8 ms | 2,323 MB | 0.116 |
| Docling CPU | 89/1269 | 185.840 s | 0.48 | 1,317.1 ms | 12,149.1 ms | 2,745 MB | 0.012 |

Docling 使用 184.933 秒上限，实际为 2 个 worker。GPU 模式平均利用率 75.0%，显存平均
4,858 MiB；CPU 模式实际 2 个 worker，没有用 GPU。评分把被截断的页按失败算，
所以 Docling 这两行是“质量 + 完成率”，不是逐页质量结论。

### OCR：olmOCR-bench old_scans + old_scans_math，134 页

| 工具 | 模式 | 完成 ↑ | wall ↓ | 页/s ↑ | 峰值内存 ↓ | 评分 ↑ | 不含 baseline ↑ |
|---|---|---:|---:|---:|---:|---:|---:|
| **yolowpdf-ocr** | GPU，版面 + 公式 | **134/134** | 882.395 s | 0.152 | 2,225 MB | **0.5428** | **0.3143** |
| yolowpdf-ocr | GPU，纯文字 | **134/134** | **59.675 s** | **2.245** | 673 MB | 0.4075 | 0.1112 |
| yolowpdf-ocr | CPU，纯文字 | **134/134** | 154.847 s | 0.865 | 616 MB | 0.4075 | 0.1112 |
| Docling OCR GPU | 默认 | 64/134 | 155.163 s | 0.41 | 4,083 MB | 0.2251 | 0.0989 |
| Docling OCR CPU | 默认 | 14/134 | 155.749 s | 0.09 | 2,543 MB | 0.0519 | 0.0257 |

公式模式使用不同时间上限和 2 个 worker，不是与受截断 Docling 运行的受控质量对比。

### 难题 PDF：pdf.js，1,089 份

| 工具 | 完成 ↑ | wall ↓ | 份/s ↑ | 峰值内存 ↓ |
|---|---:|---:|---:|---:|
| yolowpdf | 830/1089 | 0.496 s | 1,663 | 104 MB |
| pymupdf4llm | 841/1089 | 640.861 s | 1.31 | 4,094 MB |

pymupdf4llm 的 pdf.js 轮次使用 NTFS；两次运行 I/O 条件不同，不是受控对比。

## 实现

1. **文件结构**：xref 表和流、对象流、坏文件修复、加密、Flate/Brotli、JPEG、
   JPEG 2000、JBIG2、CCITT、PNG、TIFF、WebP。
2. **字体和文字**：Type1、TrueType、CFF、CID 字体、ToUnicode、预定义 CMap、
   没有 ToUnicode 的 CJK 字符集、字形宽度和文本矩阵。
3. **版面**：段落、标题、多栏阅读顺序、页眉页脚、列表、图片、页质量检查。
4. **表格和公式**：有框线、booktabs 风格横线、无框线对齐；按字形、字体、字号和
   基线重建行内 LaTeX。
5. **OCR**：核心 crate 定义 `PageOcr` trait；`yolowpdf-ocr` 用 PP-OCRv6 实现，
   可选 PP-DocLayoutV3 和 PP-FormulaNet，CPU 或 CUDA，失败时自动退回并报告原因。

核心 crate 不依赖模型运行时，OCR crate 是可选的。

## 正确性和边界

- 所有输入都有上限：输入大小、页数、单个流解压后大小、单文件时间；`page_limit`
  只处理前 N 页。
- `extract_with` 接住解析器 panic；坏文件返回 `Error`。
- 返回的文档包含扫描页检测和 OCR 状态。
- OCR 失败和部分失败会挂在返回的文档上，不会吞掉。
- `cargo test --release --workspace` 覆盖解析、字体、表格、OCR 接入、压力测试和
  fuzz 回归；fuzz 目标在 `crates/yolowpdf/fuzz`。
- `cargo fmt --check` 和 `cargo deny --config .github/deny.toml check` 通过。

## 基准

- 方法和命令：[`benchmarks/README.md`](../benchmarks/README.md)
- 工具版本和环境：[`benchmarks/tools.lock.md`](../benchmarks/tools.lock.md)
- 原始结果和完整报告：[`benchmarks/results/2026-11-09/`](../benchmarks/results/2026-11-09/)
- 数据只下载、不再分发：olmOCR-bench（ODC-BY 1.0，官方评分代码 Apache-2.0）和
  pdf.js 测试集（Apache-2.0）。

## 许可证

MIT。见 [LICENSE](../LICENSE)。第三方声明：[THIRD-PARTY.md](../THIRD-PARTY.md)。
来源说明：[UPSTREAM.md](../UPSTREAM.md)。
