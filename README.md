<div align="center">

# 🇨🇳 pizza-analysis-ik

**IK Chinese segmentation plugin for [INFINI Pizza](https://pizza.rs)**

[![Crate](https://img.shields.io/badge/crate-pizza--analysis--ik-blue)](https://github.com/pizza-rs/analysis-ik)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

</div>

---

## Overview

`pizza-analysis-ik` is a from-scratch Rust port of the popular [IK Analyzer](https://github.com/infinilabs/analysis-ik) for Chinese word segmentation. Designed for **zero-copy tokenization, compact static dictionaries, and low CPU/memory overhead**.

### Key Features

- **Smart Mode** — Non-overlapping segmentation optimized for search queries
- **Max-Word Mode** — Maximum granularity segmentation for indexing (all dictionary hits)
- **Built-in Dictionary** — Compact binary format with 270k+ entries
- **User Dictionary Support** — Extend with custom words at runtime
- **Stop Words** — Built-in Chinese + English stop word lists

## Components

| Type | Name | Description |
|:-----|:-----|:------------|
| Tokenizer | `ik_smart` | Smart mode — non-overlapping, best for queries |
| Tokenizer | `ik_max_word` | Max-word mode — all hits, best for indexing |
| Analyzer | `ik_smart` | ik_smart tokenizer → lowercase |
| Analyzer | `ik_max_word` | ik_max_word tokenizer → lowercase |

## Example

```text
Input:  "中华人民共和国国歌"
Smart:  ["中华人民共和国", "国歌"]
MaxWord: ["中华人民共和国", "中华人民", "中华", "华人", "人民共和国", "人民", "共和国", "共和", "国歌"]
```

## Installation

```toml
[dependencies]
pizza-analysis-ik = "0.1"
```

Or via `pizza-analysis-all`:

```toml
[dependencies]
pizza-analysis-all = { version = "0.1", features = ["ik"] }
```

## Usage

```rust
use pizza_engine::analysis::AnalysisFactory;

let mut factory = AnalysisFactory::new();
pizza_analysis_ik::register_all(&mut factory);
```

## License

Apache-2.0

---

<div align="center">
<sub>Part of the <a href="https://pizza.rs">INFINI Pizza</a> ecosystem</sub>
</div>
