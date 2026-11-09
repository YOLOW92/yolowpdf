#!/usr/bin/env bash
# Download the public benchmark data (not redistributed).
set -euo pipefail
DATA=${YOLOWPDF_DATA:-/home/yolow92/yolowpdf-data}
PY=${YOLOWPDF_HF_PY:-$HOME/yolowpdf-bench/py/bin/python}
mkdir -p "$DATA"
"$PY" - <<'PY'
from huggingface_hub import snapshot_download
snapshot_download(repo_id="allenai/olmOCR-bench", repo_type="dataset",
                  revision="54a96a6fb6a2bd3b297e59869491db4d3625b711",
                  local_dir=f"{__import__('os').environ.get('YOLOWPDF_DATA','/home/yolow92/yolowpdf-data')}/olmocr_1403",
                  allow_patterns=["bench_data/**"], max_workers=12)
PY
if [ ! -d "$DATA/pdfjs/.git" ]; then
  git clone --depth 1 --filter=blob:none --sparse https://github.com/mozilla/pdf.js.git "$DATA/pdfjs"
fi
cd "$DATA/pdfjs"
git sparse-checkout set test/pdfs
git show HEAD:test/test_manifest.json > test/test_manifest.json
git rev-parse HEAD
