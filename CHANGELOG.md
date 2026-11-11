# Changelog

All notable changes are listed here. The project follows [Semantic Versioning](https://semver.org/).

## 0.1.1

PDF decoding now enforces its resource budget before allocation.

- **Predictors:** PNG/TIFF predictor row buffers are checked against the decode limit before `prev`/`cur` are allocated; `Columns`, `Colors` and `BitsPerComponent` arithmetic is checked, and the TIFF 1/2/4-bit component table is included in the budget.
- **Filters:** ASCII85, ASCIIHex, RunLength and Crypt verify the output limit before reserving, extending or resizing buffers, and return `TooLarge` instead of truncating.
- **Compatibility:** valid PDFs, including PNG filters 0–4 and TIFF 1/2/4/8/16-bit samples, decode to the same bytes. Corrupt inputs that previously parsed by accident can now return a format or size error.
