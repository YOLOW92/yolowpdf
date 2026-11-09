# Third-party notices

yolowpdf is MIT-licensed. It includes ideas, rules, data and model formats from
the projects below. No source code from them is included unless stated.

## Algorithms and data

- **pdfplumber** (https://github.com/jsvine/pdfplumber), MIT License,
  Copyright (c) 2015 Jeremy Singer-Vine.
  The lattice table algorithm (snapping and joining ruling lines,
  intersections, smallest cells, grouping cells into tables) is reimplemented
  in `crates/yolowpdf/src/table/`.

- **pdfminer.six** (https://github.com/pdfminer/pdfminer.six), MIT License,
  Copyright (c) 2012-2024 PDFMiner contributors.
  The CFF standard strings from Adobe Technical Note #5176, Appendix A, are
  used in `crates/yolowpdf/src/fontfile/`.

- **Adobe cmap-resources** (https://github.com/adobe-type-tools/cmap-resources),
  BSD 3-Clause License, Copyright 1990-2019 Adobe. All rights reserved.
  The CID-to-Unicode tables for Adobe-GB1, Adobe-CNS1, Adobe-Japan1 and
  Adobe-Korea1 (`cid2code.txt`) are converted to the tables under
  `crates/yolowpdf/src/font/cid2uni/`, and used to decode CJK text in PDFs whose
  CID fonts carry no ToUnicode map. The full BSD notice is reproduced in
  `crates/yolowpdf/src/font/cid2uni/` and in the upstream NOTICE.

## Models (downloaded, not redistributed)

The OCR crate downloads these model files from Hugging Face on first use and
verifies their size and SHA-256. They are not part of this repository.

- PaddlePaddle **PP-OCRv6** detection and recognition models, Apache-2.0.
- PaddlePaddle **PP-DocLayoutV3** layout model, Apache-2.0.
- PaddlePaddle **PP-FormulaNet_plus-L** formula model, Apache-2.0.

## Benchmark data

- **olmOCR-bench** (https://huggingface.co/datasets/allenai/olmOCR-bench),
  ODC-BY 1.0, and the scoring code from
  https://github.com/allenai/olmocr, Apache-2.0. Downloaded by the benchmark;
  not redistributed.
- **pdf.js** test corpus (https://github.com/mozilla/pdf.js), Apache-2.0.
  Downloaded by the benchmark; not redistributed.
