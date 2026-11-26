//! 模型清单：从哪下载、锁定的上游版本、每个文件的大小和 SHA-256；本地缓存目录与校验。
//! 权重是 PaddlePaddle 在 Hugging Face 上发布的 PP-OCRv6 safetensors（Apache-2.0），不随本仓库
//! 分发，打开 OCR 后第一次用时下载（下载由调用方用自己的 HTTP 客户端做，这里只给地址和校验）。

use std::io::Read;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// 一个模型文件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct File {
    pub name: &'static str,
    pub size: u64,
    /// 小写十六进制的 SHA-256。
    pub sha256: &'static str,
}

/// 一个模型（一个 Hugging Face 仓库的一个锁定版本）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Model {
    /// 仓库名，也是本地缓存里的目录名。
    pub repo: &'static str,
    /// 锁定的提交号：内容固定，校验值才有意义。
    pub revision: &'static str,
    pub files: &'static [File],
}

/// [协议] Hugging Face 的组织名
const ORG: &str = "PaddlePaddle";

impl Model {
    /// 文件的下载地址：
    /// - `repo` 不含 `/` 时按上游默认组织拼接：`https://huggingface.co/PaddlePaddle/<repo>/resolve/<rev>/<file>`
    /// - `repo` 含 `/` 时前两段为 `<org>/<repo>`，其余段为仓库内子目录：
    ///   `https://huggingface.co/<org>/<repo>/resolve/<rev>/[<subdir>/]<file>`
    pub fn url(&self, file: &File) -> String {
        let mut parts = self.repo.splitn(3, '/');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(org), Some(repo), Some(sub)) if !sub.is_empty() => format!(
                "https://huggingface.co/{org}/{repo}/resolve/{}/{sub}/{}",
                self.revision, file.name
            ),
            (Some(org), Some(repo), _) => format!(
                "https://huggingface.co/{org}/{repo}/resolve/{}/{}",
                self.revision, file.name
            ),
            _ => format!(
                "https://huggingface.co/{ORG}/{}/resolve/{}/{}",
                self.repo, self.revision, file.name
            ),
        }
    }

    /// 本地目录：`<缓存根>/<仓库名>`。
    pub fn dir(&self, root: &Path) -> PathBuf {
        root.join(self.repo)
    }

    /// 所有文件都在且大小对（不算哈希，载入模型前用）。
    pub fn present(&self, root: &Path) -> bool {
        let dir = self.dir(root);
        self.files.iter().all(|f| {
            std::fs::metadata(dir.join(f.name)).is_ok_and(|m| m.is_file() && m.len() == f.size)
        })
    }

    /// 本地目录里的文件是否齐全、大小和 SHA-256 是否都对；返回第一处问题。
    pub fn verify(&self, root: &Path) -> Result<(), String> {
        let dir = self.dir(root);
        for f in self.files {
            let p = dir.join(f.name);
            let mut hash = Hasher::new(f);
            let mut file = std::fs::File::open(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            let mut buf = vec![0u8; READ_CHUNK];
            loop {
                let n = file
                    .read(&mut buf)
                    .map_err(|e| format!("{}: {e}", p.display()))?;
                if n == 0 {
                    break;
                }
                hash.update(&buf[..n]);
            }
            hash.finish().map_err(|e| format!("{}: {e}", p.display()))?;
        }
        Ok(())
    }
}

/// [策略] 校验时每次读的字节数
const READ_CHUNK: usize = 1 << 20;

/// 边下载边校验一个文件（模型文件上百 MB，不整个放进内存）。
pub struct Hasher<'a> {
    file: &'a File,
    sha: Sha256,
    size: u64,
}

impl<'a> Hasher<'a> {
    pub fn new(file: &'a File) -> Self {
        Self {
            file,
            sha: Sha256::new(),
            size: 0, // [协议] 上游文件大小
        }
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.sha.update(bytes);
        self.size += bytes.len() as u64;
    }

    /// 大小和 SHA-256 都对才算过；返回问题描述。
    pub fn finish(self) -> Result<(), String> {
        if self.size != self.file.size {
            return Err(format!("{} bytes, expected {}", self.size, self.file.size));
        }
        let got: String = self
            .sha
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if got != self.file.sha256 {
            return Err(format!("SHA-256 is {got}, expected {}", self.file.sha256));
        }
        Ok(())
    }
}

/// 校验下载来的一个文件。
pub fn check(f: &File, bytes: &[u8]) -> Result<(), String> {
    let mut h = Hasher::new(f);
    h.update(bytes);
    h.finish()
}

/// 缓存根目录：`$XDG_CACHE_HOME/yolowpdf/models`，没设时 `~/.cache/yolowpdf/models`
/// （XDG Base Directory 规范）。
pub fn cache_root() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))?;
    Some(base.join("yolowpdf").join("models"))
}

// [协议] 以下大小和 SHA-256 取自各仓库锁定提交的文件（2026-11-03 核对：safetensors 的值与
// Hugging Face LFS 指针一致，JSON 文件按下载内容计算）

/// CPU 用的小模型：检测。
pub const SMALL_DET: Model = Model {
    repo: "PP-OCRv6_small_det_safetensors",
    revision: "eae2ee920a39fb3087637d3dbb58df1896ec1f24",
    files: &[
        File {
            name: "model.safetensors",
            size: 9_938_124, // [协议] 上游文件大小
            sha256: "89a96a8adc4e9cd0c994098edc76022e496d35844392562b4694c8fbc583f2da",
        },
        File {
            name: "config.json",
            size: 1_096, // [协议] 上游文件大小
            sha256: "b42d76ab6325d5234e1afc9987b73dc592b0ec8437c8a29d46ba6209b79252eb",
        },
        File {
            name: "preprocessor_config.json",
            size: 838, // [协议] 上游文件大小
            sha256: "74421569a28a78d417db320f9bc039ce0997a1defe2b35f259dfc74299c9f1ed",
        },
    ],
};

/// CPU 用的小模型：识别。
pub const SMALL_REC: Model = Model {
    repo: "PP-OCRv6_small_rec_safetensors",
    revision: "fe049fb103f57443fe8840c54ed06b702f3c1de5",
    files: &[
        File {
            name: "model.safetensors",
            size: 21_204_736, // [协议] 上游文件大小
            sha256: "f65a332afe5aa663f0b9d5706f4ae8457b5b4058a842d5c1eb22df505c27d642",
        },
        File {
            name: "config.json",
            size: 1_083, // [协议] 上游文件大小
            sha256: "8693fd8485e8543e13d0f6dde3891a3f4af9a47fc4e3a391fbed71f8015c899e",
        },
        File {
            name: "preprocessor_config.json",
            size: 206_177, // [协议] 上游文件大小
            sha256: "2b24fa36f548893f26a931cf44f1a1a6b2b14ed7e2f8f8e28e6848801a8278db",
        },
    ],
};

/// GPU 用的中模型：检测。
pub const MEDIUM_DET: Model = Model {
    repo: "PP-OCRv6_medium_det_safetensors",
    revision: "4236c2b61741a259c091fd879dcc4edc339e916c",
    files: &[
        File {
            name: "model.safetensors",
            size: 88_020_412, // [协议] 上游文件大小
            sha256: "bd393266c02e1a680b1b34c301d5d0d81e6290440b7f8ab0f5d5032276b17eb1",
        },
        File {
            name: "config.json",
            size: 2_134, // [协议] 上游文件大小
            sha256: "3ea7e760f64255c152ba139e2f5f798303f7189b87c923cf498e2b2e830cadd3",
        },
        File {
            name: "preprocessor_config.json",
            size: 838, // [协议] 上游文件大小
            sha256: "74421569a28a78d417db320f9bc039ce0997a1defe2b35f259dfc74299c9f1ed",
        },
    ],
};

/// GPU 用的中模型：识别。
pub const MEDIUM_REC: Model = Model {
    repo: "PP-OCRv6_medium_rec_safetensors",
    revision: "024cad6a831de75c2c3c26e711ba8c4a82ccd24b",
    files: &[
        File {
            name: "model.safetensors",
            size: 76_741_720, // [协议] 上游文件大小
            sha256: "5f43c16f2a684b1d2284662178bdb604febd3d6bfdb5ca73828d08d0f7c0c3e9",
        },
        File {
            name: "config.json",
            size: 1_131, // [协议] 上游文件大小
            sha256: "aa5f351b4a7df95cc4953f013d53a6d44205f1bb0aee0ec7eb243d1d8fae1c27",
        },
        File {
            name: "preprocessor_config.json",
            size: 206_177, // [协议] 上游文件大小
            sha256: "2b24fa36f548893f26a931cf44f1a1a6b2b14ed7e2f8f8e28e6848801a8278db",
        },
    ],
};

/// 公式识别（只有 L 级有 safetensors）。
pub const FORMULA: Model = Model {
    repo: "PP-FormulaNet_plus-L_safetensors",
    revision: "6cc004a8e91ea3f3819b05317f40b290cbe99c2b",
    files: &[
        File {
            name: "model.safetensors",
            size: 727_633_048, // [协议] 上游文件大小
            sha256: "582848cf9b7066bb1cf3166ad75a476da492c3661ffa61bb7439f9fd098bde13",
        },
        File {
            name: "config.json",
            size: 1_364, // [协议] 上游文件大小
            sha256: "3d5c513a525ebef73ba4e319e65de793d95aacc1062e5ce2dd851c7f109c060c",
        },
        File {
            name: "generation_config.json",
            size: 186, // [协议] 上游文件大小
            sha256: "e0dc49736b0b06f19cb2074201a072cee23707b7abff4a55a24da19b14abd707",
        },
        File {
            name: "processor_config.json",
            size: 584, // [协议] 上游文件大小
            sha256: "f1454999fcb5930c2d93e9dcd03f1adf046245e98e96a7f601dabc6ee01c3b78",
        },
        File {
            name: "tokenizer.json",
            size: 2_140_014, // [协议] 上游文件大小
            sha256: "2811d82701ec97c192fa256aa2b4516929373870ae660326cc5b1dc879b95ff2",
        },
        File {
            name: "tokenizer_config.json",
            size: 4_484, // [协议] 上游文件大小
            sha256: "0bf3cca5ff1a240004932dbab09373b0e8a651f5f85f2ff6f2f9dec3f0e41947",
        },
    ],
};

/// 版面检测（找公式区域）。
pub const LAYOUT: Model = Model {
    repo: "PP-DocLayoutV3_safetensors",
    revision: "e4f489dc536556fc1be02b973ba06756e0d4a2ac",
    files: &[
        File {
            name: "model.safetensors",
            size: 133_270_468, // [协议] 上游文件大小
            sha256: "5ea422c6cc5fe759a47e1357c35639b58173508e025a3131cbe4b6ac59e2b85e",
        },
        File {
            name: "config.json",
            size: 2_460, // [协议] 上游文件大小
            sha256: "3cf834b91d23a756b1519bce4db42c09e852f3e35c35092dd5a3e253a50c071a",
        },
        File {
            name: "inference.yml",
            size: 1_482, // [协议] 上游文件大小
            sha256: "506fcfac13b3b546ae40d7886b44126420f392adb694e3f8bb6a6286a1f90fdc",
        },
        File {
            name: "preprocessor_config.json",
            size: 575, // [协议] 上游文件大小
            sha256: "519fe0187a43a1ca429e3ad8317bab8700f0d5e8fb3a6e3a0a413ffac078ba42",
        },
    ],
};

// [协议] YolowOCR-Suite v2（MIT，自研从零训练 BF16 权重，2026-11-26 发布至 YOLOW92/yolowocr-v2）

/// YolowOCR-Suite v2 锁定提交号。
const V2_REV: &str = "7cf4d7ae36f1dde30ce492b3dfbbf32460fdd0e7";

/// 自研轻量文字检测（5.05 MB BF16）。
pub const V2_DET: Model = Model {
    repo: "YOLOW92/yolowocr-v2/det_v2",
    revision: V2_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 5_053_492,
            sha256: "b2f88de4dc5493140644f3d0e24caa7789d0509013fef7820e8406685dd0ae86",
        },
        File {
            name: "config.json",
            size: 1_845,
            sha256: "f6c244c467ba644c30458713d1c7d21db04fcfcb0c8849f30fc6c18cc15bac9a",
        },
        File {
            name: "preprocessor_config.json",
            size: 903,
            sha256: "3623b5618fb94fbb5ea3f6b4d1e490052e30c3ad1c4a35c1d5b1a56dfd3131ca",
        },
    ],
};

/// 自研多语种文字识别（10.74 MB BF16，18,710 字符表）。
pub const V2_REC_MULTI: Model = Model {
    repo: "YOLOW92/yolowocr-v2/rec_v2_multi",
    revision: V2_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 10_738_464,
            sha256: "aeaac2cec0cb546e8c82b9498a05ae1f524db4dd2a451419bacec90b7755ae2c",
        },
        File {
            name: "config.json",
            size: 1_864,
            sha256: "20d34208757dea0f7bf16aa3c086f3436b003f287d8bc791c331956978b0d608",
        },
        File {
            name: "preprocessor_config.json",
            size: 206_202,
            sha256: "bd7033aae58d809ce01bcfca9c29eae9ae193dd79a69b7233eb1dfa0a9e68a8f",
        },
    ],
};

/// 自研英文/拉丁轻量文字识别（6.24 MB BF16，254 字符表）。
pub const V2_REC_LATIN: Model = Model {
    repo: "YOLOW92/yolowocr-v2/rec_v2_latin",
    revision: V2_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 6_235_008,
            sha256: "889be30a562b22c55bf8be5c24dab8089109fc80e835c2a4f1ec3bf932594b74",
        },
        File {
            name: "config.json",
            size: 1_862,
            sha256: "c80b98458557d0d8d4b71818162c618433590c6d4d51099808618b72ab950801",
        },
        File {
            name: "preprocessor_config.json",
            size: 2_764,
            sha256: "9a1495170a2efdd0660afff7980d0194dd709385c4ec824086aa0940e338c568",
        },
    ],
};

/// 自研轻量版面检测（26.81 MB BF16，640×640，3 层解码器）。
pub const V2_LAYOUT: Model = Model {
    repo: "YOLOW92/yolowocr-v2/layout_s_v2",
    revision: V2_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 26_812_404,
            sha256: "e5113166f6167a4b2d617f443f785188474bf3ce12b9fc4bfe24888c9159f944",
        },
        File {
            name: "config.json",
            size: 3_120,
            sha256: "0b0c58bcf9abe041122493f726e2a8e356f0414dc46d3cae1d38ca1dee388d53",
        },
        File {
            name: "preprocessor_config.json",
            size: 576,
            sha256: "84dd88ea533f6321c7ab3da4faad7462c2b5dd8a46e64b533ab08c5679687930",
        },
        File {
            name: "inference.yml",
            size: 446,
            sha256: "dd885556a09963173b88ceedf1fb802a08a0828e18d247e45fa06982db790872",
        },
    ],
};

/// 自研矩形公式识别 + MTP-4 无损自投机解码（66.99 MB BF16，224×672）。
pub const V2_FORMULA: Model = Model {
    repo: "YOLOW92/yolowocr-v2/formula_v2_rect_mtp",
    revision: V2_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 66_986_280,
            sha256: "625e92249a37222bb5a48ad40e02419eaaff0225354ef4238795e0810156722b",
        },
        File {
            name: "config.json",
            size: 1_138,
            sha256: "2014c62c5bf1c28d8b5cae8c58db2e5e87f724d7c37231747be041a61df2b19d",
        },
        File {
            name: "generation_config.json",
            size: 168,
            sha256: "f32c3530b9502ff9f28611b762c9da605e075643f8b023d6bcafb28e60004960",
        },
        File {
            name: "preprocessor_config.json",
            size: 373,
            sha256: "22e42e223c2036bc28d0c0636b19091fdda0e55d516a88474dddd80fc5b49786",
        },
        File {
            name: "processor_config.json",
            size: 373,
            sha256: "22e42e223c2036bc28d0c0636b19091fdda0e55d516a88474dddd80fc5b49786",
        },
        File {
            name: "tokenizer.json",
            size: 111_210,
            sha256: "d1e711af309bcfef0b7783c8192320d45dd7813504cf8b3e527c0040c555ba12",
        },
    ],
};

// [协议] YolowOCR-Suite v3（MIT，自研大参数 BF16 权重，2026-11-27 发布至 YOLOW92/yolowocr-v3）
// 其中 rec_v3_large（38.4k 步）与 rec_v3_latin_large（18k 步）已完整训完并接入默认高精档；
// det_v3_large（6.6k/24k）、layout_l_v3（4.3k/24k）、formula_v3_pro（15.8k/35k）为未训满预览检查点。

/// YolowOCR-Suite v3 锁定提交号。
const V3_REV: &str = "b45edbf6feeb7a2f576dc40bbb8b16451c0f45b1";

/// 自研高精多语种文字识别（44.72 MB BF16，LCNetV4-L + 4L SVTR h=256，18,710 字符表，已完整训完）。
pub const V3_REC_MULTI: Model = Model {
    repo: "YOLOW92/yolowocr-v3/rec_v3_large",
    revision: V3_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 44_722_472,
            sha256: "240977353edb7cf91418f344e34a486a09bc8188ec2bef35113b140adbede54a",
        },
        File {
            name: "config.json",
            size: 1_992,
            sha256: "f9d367f34312d6ec068d395a8649894c24856281eac761bd3e157b20d6f22fcd",
        },
        File {
            name: "preprocessor_config.json",
            size: 206_202,
            sha256: "bd7033aae58d809ce01bcfca9c29eae9ae193dd79a69b7233eb1dfa0a9e68a8f",
        },
    ],
};

/// 自研高精英文/拉丁文字识别（35.20 MB BF16，LCNetV4-L + 4L SVTR h=256，254 字符表，已完整训完）。
pub const V3_REC_LATIN: Model = Model {
    repo: "YOLOW92/yolowocr-v3/rec_v3_latin_large",
    revision: V3_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 35_199_064,
            sha256: "fc453cc01ad248801a27561ddbde559c46636e6934e3ce5e2f2d1a48ad4b4e08",
        },
        File {
            name: "config.json",
            size: 1_990,
            sha256: "1aeb2b276a5724e2683441626871ff2e2ee8eb39c5fddab5fb67151daef59848",
        },
        File {
            name: "preprocessor_config.json",
            size: 2_764,
            sha256: "9a1495170a2efdd0660afff7980d0194dd709385c4ec824086aa0940e338c568",
        },
    ],
};

/// 自研大参数文字检测预览检查点（44.20 MB BF16，6.6k/24k 步，未训满）。
pub const V3_DET: Model = Model {
    repo: "YOLOW92/yolowocr-v3/det_v3_large",
    revision: V3_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 44_204_012,
            sha256: "fa80a4a03f15b0d959531868751c506e0ab3ae31271f7d45e2a6d62d1ed80c64",
        },
        File {
            name: "config.json",
            size: 3_747,
            sha256: "17e73e68fa867462ff3c013df28f7ad0087efeee87dfe848bd817f4fe055d8c1",
        },
        File {
            name: "preprocessor_config.json",
            size: 903,
            sha256: "3623b5618fb94fbb5ea3f6b4d1e490052e30c3ad1c4a35c1d5b1a56dfd3131ca",
        },
    ],
};

/// 自研满血版面检测预览检查点（67.03 MB BF16，800×800，4.3k/24k 步，未训满）。
pub const V3_LAYOUT: Model = Model {
    repo: "YOLOW92/yolowocr-v3/layout_l_v3",
    revision: V3_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 67_034_116,
            sha256: "f5a82712c8bf515db5058a522cebb1b7d2166e0b7f69106b6b5e2a04909ac4af",
        },
        File {
            name: "config.json",
            size: 2_568,
            sha256: "779f68204df00965006c1251c6ae9185dd8d58d982795e0c136cde9c9ecba613",
        },
        File {
            name: "preprocessor_config.json",
            size: 576,
            sha256: "7b3b2030cc6ad3bbc67d4a80377e9d75c8dad45d49d01df260de214bdf7ce5d5",
        },
        File {
            name: "inference.yml",
            size: 446,
            sha256: "697b475edc9a06ed30629fedc329f09b13100fac83b9e13aaf138ebc03f1cda0",
        },
    ],
};

/// 自研 127M Pro 公式识别预览检查点（255.20 MB BF16，256×768 + MTP-4，15.8k/35k 步，未训满）。
pub const V3_FORMULA: Model = Model {
    repo: "YOLOW92/yolowocr-v3/formula_v3_pro",
    revision: V3_REV,
    files: &[
        File {
            name: "model.safetensors",
            size: 255_195_152,
            sha256: "cdd66cfa8b89dfd0d947471ff4f55a1d48f8fba809c55727e9c5796afb01afe2",
        },
        File {
            name: "config.json",
            size: 1_159,
            sha256: "3a34a736b856e7f6cd5c451812cddbb8321c752e4d38ec99d5c0de729d6e9380",
        },
        File {
            name: "generation_config.json",
            size: 168,
            sha256: "f32c3530b9502ff9f28611b762c9da605e075643f8b023d6bcafb28e60004960",
        },
        File {
            name: "preprocessor_config.json",
            size: 373,
            sha256: "6b07fbe6898f8aca0036be76c805d47dd3f1bf84c3e5318d8d035e51dcf798ff",
        },
        File {
            name: "processor_config.json",
            size: 373,
            sha256: "6b07fbe6898f8aca0036be76c805d47dd3f1bf84c3e5318d8d035e51dcf798ff",
        },
        File {
            name: "tokenizer.json",
            size: 111_210,
            sha256: "d1e711af309bcfef0b7783c8192320d45dd7813504cf8b3e527c0040c555ba12",
        },
    ],
};

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机缓存里已有的模型要能通过校验（没有缓存时跳过）。
    #[test]
    fn cached_models_verify() {
        let Some(root) = cache_root() else { return };
        for m in [
            SMALL_DET,
            SMALL_REC,
            MEDIUM_DET,
            MEDIUM_REC,
            FORMULA,
            LAYOUT,
            V2_DET,
            V2_REC_MULTI,
            V2_REC_LATIN,
            V2_LAYOUT,
            V2_FORMULA,
            V3_REC_MULTI,
            V3_REC_LATIN,
            V3_DET,
            V3_LAYOUT,
            V3_FORMULA,
        ] {
            if m.dir(&root).join("model.safetensors").exists() {
                m.verify(&root).unwrap();
            }
        }
    }

    #[test]
    fn resolves_upstream_and_subdir_urls() {
        assert_eq!(
            SMALL_DET.url(&SMALL_DET.files[1]),
            format!(
                "https://huggingface.co/PaddlePaddle/PP-OCRv6_small_det_safetensors/resolve/{}/config.json",
                SMALL_DET.revision
            )
        );
        assert_eq!(
            V2_DET.url(&V2_DET.files[0]),
            format!(
                "https://huggingface.co/YOLOW92/yolowocr-v2/resolve/{V2_REV}/det_v2/model.safetensors"
            )
        );
        assert_eq!(
            V3_REC_MULTI.url(&V3_REC_MULTI.files[0]),
            format!(
                "https://huggingface.co/YOLOW92/yolowocr-v3/resolve/{V3_REV}/rec_v3_large/model.safetensors"
            )
        );
        assert_eq!(
            V2_FORMULA.dir(Path::new("/cache")),
            PathBuf::from("/cache/YOLOW92/yolowocr-v2/formula_v2_rect_mtp")
        );
    }

    #[test]
    fn rejects_wrong_bytes() {
        let f = &SMALL_DET.files[1];
        assert!(check(f, b"{}").is_err());
    }
}
