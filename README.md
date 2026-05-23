<div align="center">

# 🇨🇳 pizza-analysis-ik

**IK Chinese segmentation plugin for [INFINI Pizza](https://pizza.rs)**

[![Crate](https://img.shields.io/badge/crate-pizza--analysis--ik-blue)](https://github.com/pizza-rs/analysis-ik)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](LICENSE)

</div>

---

A from-scratch Rust port of the [`pizza-rs/analysis-ik`](https://github.com/pizza-rs/analysis-ik)
Java plugin. Designed for **zero-copy tokenization, compact static dictionaries,
and low CPU / memory overhead**, with first-class support for runtime extension
via [`Rules`].

## Highlights

- **Bundled, compact dictionaries.** `main.dic` (~275 K entries),
  `quantifier.dic`, and `stopword.dic` are baked into the binary via
  `build.rs` as concatenated UTF-8 blobs plus binary u32 offset tables — no
  filesystem I/O, no startup parsing, no nested HashMap trie.
- **Zero-copy hot path.** Tokens borrow directly from the input UTF-8 string
  whenever regularization is a no-op (the common case for pure CJK / pure
  ASCII text). Only ranges that change shape (full-width → half-width,
  case-folding) are materialized.
- **Two modes, matching the Java plugin:**
  - [`IkMode::Smart`] = `ik_smart` — DP arbitrator picks one best path
    (good for queries).
  - [`IkMode::MaxWord`] = `ik_max_word` — emits every dict hit, possibly
    overlapping (good for indexing).
- **Runtime extensions** via [`Rules`]: add words, add stopwords, mask
  bundled words — without rebuilding the crate.
- **Pure Rust, no `unsafe` on the public API.** `Send + Sync`; cheap to
  clone and share across threads.
- **Implements `pizza_engine::analysis::Tokenizer`** — drop-in for any
  pipeline using the pizza engine.

## Installation

```toml
[dependencies]
pizza-ik = { path = "../contrib/ik" }
```

The default feature set bundles `main.dic`. To save ~2 MB of binary size at
the cost of dropping the main word list (only quantifier / stopword
support, useful for tests), disable default features:

```toml
pizza-ik = { path = "../contrib/ik", default-features = false }
```

## Quick start

```rust
use pizza_engine::analysis::Tokenizer;
use pizza_ik::{IkConfig, IkMode, IkTokenizer};

// ik_smart — single best segmentation (great for queries).
let tk = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart));
for tok in tk.tokenize("中华人民共和国成立于1949年") {
    println!("{} [{}..{}]", &*tok.term, tok.start_offset, tok.end_offset);
}
// → 中华人民共和国 [0..21]
//   成立           [21..27]
//   1949           [30..34]
//   年             [34..37]

// ik_max_word — all overlapping hits (great for indexing).
let tk = IkTokenizer::new(IkConfig::default().mode(IkMode::MaxWord));
let tokens: Vec<_> = tk
    .tokenize("中华人民共和国成立于1949年")
    .into_iter()
    .map(|t| t.term.into_owned())
    .collect();
// → ["中华人民共和国","中华人民","中华","中","华人","人民共和国","人民",
//    "共和国","共和","成立","立于","1949","年"]
```

## `IkMode` comparison

| Mode      | Behavior                                              | Use for              |
| --------- | ----------------------------------------------------- | -------------------- |
| `Smart`   | Best non-overlapping path via DP arbitration          | Query analysis       |
| `MaxWord` | Every dict hit (overlapping), plus uncovered CJK chars | Index analysis      |

`Smart` also fuses adjacent Chinese number + quantifier into a single
`CnQuan` lexeme (e.g. `三个` → one token).

### Subset invariant: `Smart ⊆ MaxWord`

For every input, the set of tokens emitted by `Smart` (compared as
`(term, start_offset, end_offset)`) is a **subset** of the tokens emitted
by `MaxWord`. This is the contract behind the standard pattern:

- **Index** with `MaxWord` → cast a wide net, every reasonable token is
  indexed.
- **Query** with `Smart` → emit the disambiguated minimal set, every one
  of which is guaranteed to exist in the index.

To uphold this invariant, `MaxWord` additionally:
1. Emits every quantifier fusion `Smart` would produce (e.g. `三个`,
   `五公斤`) **alongside** the constituent `CnNum` + `Count` tokens.
2. Force-emits a single-char `CnChar` token at every Chinese position,
   even when covered by a longer word, so any single char `Smart` might
   add after arbitration is already present in the index.

The invariant is exercised by the
`tokenizer::tests::smart_is_subset_of_maxword` test.

## `IkConfig` reference

| Field           | Type     | Default     | Description |
| --------------- | -------- | ----------- | ----------- |
| `mode`          | `IkMode` | `MaxWord`   | `Smart` or `MaxWord`. |
| `lowercase`     | `bool`   | `true`      | Lowercase ASCII (and full-width ASCII after regularization) when emitting tokens. |
| `use_stopwords` | `bool`   | `false`     | Drop tokens that match the bundled stopword dictionary (or any user-added stopword). |
| `ensure_smart_subset` | `bool` | `true` | Only affects `MaxWord`. When `true`, emit the extra `CnQuan` fusions and per-char `CnChar` tokens needed to guarantee `Smart ⊆ MaxWord`. Disable for a smaller/faster index when you do not need that guarantee. |

Convenience presets:

```rust
use pizza_ik::IkConfig;

let cfg_index  = IkConfig::for_indexing();   // MaxWord, lowercase, no stopwords
let cfg_search = IkConfig::for_searching();  // Smart,   lowercase, stopwords on
let cfg_fast_index = IkConfig::for_indexing().ensure_smart_subset(false);
```

Builder methods (`.mode()`, `.lowercase()`, `.use_stopwords()`,
`.ensure_smart_subset()`) all consume and return `IkConfig`, so they chain.

## Runtime rules

[`Rules`] is a small overlay applied on top of the bundled dictionaries.
Use it to teach IK new words, register custom stopwords, or veto an unwanted
bundled match — all at runtime.

```rust
use pizza_ik::{IkTokenizer, Rules};

let mut rules = Rules::new();
rules
    .add_word("披萨")               // new main-dict entry
    .add_word("披萨饼")
    .add_stopword("的")             // also drop "的" when use_stopwords=true
    .remove_word("中华")            // veto a bundled word
    .remove_stopword("the");        // unblock a bundled stopword

let tk = IkTokenizer::with_defaults().with_rules(rules);
```

You can also keep adding rules after construction:

```rust
let mut tk = IkTokenizer::with_defaults();
tk.rules_mut().add_words(["云原生", "向量数据库"]);
```

## Dictionary inspection

The bundled dictionaries are queryable via the `Dict` API without
instantiating a tokenizer:

```rust
use pizza_ik::Dict;

assert!(Dict::contains_main("中国"));
assert!(Dict::contains_quantifier("个"));
assert!(Dict::is_stopword("the"));
println!("main words: {}", Dict::main_size());
```

All lookups are `O(log N)` (first-char bucket + binary search) and never
allocate.

## Character coverage

- **CJK Unified Ideographs** (U+4E00–U+9FFF), CJK Ext-A (U+3400–U+4DBF),
  and CJK Compatibility Ideographs (U+F900–U+FAFF) → `Chinese`.
- **Hiragana / Katakana / Hangul / Halfwidth-Fullwidth Forms** →
  `OtherCjk` (segmented as standalone tokens by `MaxWord`, kept as a run
  by `Smart`).
- **ASCII letters / digits + connectors `# & + - . @ _` / `, .`** are
  grouped into English, Arabic, or mixed `Letter` lexemes, mirroring the
  Java `LetterSegmenter`.
- **Full-width ASCII** (U+FF01–U+FF5E) and the ideographic space U+3000 are
  regularized to half-width before classification.

## Performance notes

The hot lookup path is a **compile-time char-level prefix trie** of the
main dictionary, packed as three `&'static [u8]` blobs in CSR layout
(`nodes`, `edge_char`, `edge_next`), plus a **root direct-lookup table**
that maps the ~5 K root codepoints to their child-node ids in one array
load. Subsequent (inner) steps remain `O(log fanout)` binary searches
over node-local edges (fanout typically 1–5 inside the dictionary). The
terminal flag is stored in bit 31 of each node entry, so prefix emission
is branch-cheap and the matched char count is produced *during* the
walk — no second `utf8_char_count` pass.

Trie reads use **unaligned `u64` / `u32` loads** (`core::ptr::read_unaligned`)
rather than `from_le_bytes` over an array literal — one `mov` /
`ldur` instruction per step instead of four byte loads and a shift
chain. `node_info` fuses the two adjacent `u32` reads of `(current,
next)` into one unaligned `u64` load. On the streaming MaxWord and
`cjk_segment` paths the trie is walked over the pre-decoded `&[char]`
slice from `CharStream`, skipping the per-position `text.chars()`
UTF-8 re-decode entirely.

`CharStream::new` uses a fused `regularize_and_classify` helper with a
fast path for the **CJK Unified Ideographs** block (U+4E00..=U+9FFF):
characters in that range are invariant under `regularize` and always
classify as `Chinese`, so one compare-and-branch replaces the ~6
conditional range checks that the unfused two-pass version performed
on every codepoint.

Measured on Apple Silicon (M-series), release build, 1 740-char Chinese
text × 1 000 iterations, median of 5 runs:

| Workload                          | Throughput            |
| --------------------------------- | --------------------- |
| MaxWord (no subset guarantee)     | **~8.6 M chars / s**  |
| MaxWord (Smart ⊆ MaxWord on)      | **~7.5 M chars / s**  |
| Smart                             | **~6.9 M chars / s**  |

Most CJK ranges produce **zero allocations** — emitted tokens borrow
directly from the input via `Cow::Borrowed`.

### Streaming MaxWord & bucket-sorted Smart

The MaxWord tokenizer is **streaming**: instead of materializing a global
`Vec<Lexeme>`, sorting it, deduping, and then iterating to emit tokens,
it walks the input one position at a time, fills a small reused per-
position bucket (CJK trie hits + precomputed letter / quantifier hits +
optional forced singleton), dedups inline via a linear scan, and pushes
tokens directly. This eliminates the `O(K log K)` global sort on the
hot path; only the sparse "aux" precompute (letter + quantifier) is
sorted once.

The Smart tokenizer's post-arbitration sort is now an `O(N + K)`
counting sort keyed on `begin`, with per-bucket comparison sort over a
handful of entries. This replaces the `sort_unstable_by_key` on the
final lexeme set.

`Lexeme`'s ordering key is a packed `u64`
(`begin << 32 | (0xFF_FFFF - length) << 8 | priority`), so any
remaining sorts are integer-key compares with branch-free SIMD-friendly
inner loops.

Empty-set fast paths in `Rules` (`has_extra_words`, `has_removed_main`)
let the per-position CJK loop skip the user-extra `BTreeSet::range`
walk and the per-trie-hit term-slice construction whenever the user
hasn't configured any extras or removals — the overwhelmingly common
case.

Memory footprint of the bundled dictionaries:

- Main dict trie: ~3 MB (`nodes`) + ~3 MB (`edge_char`) + ~3 MB
  (`edge_next`) ≈ **~9 MB** for the full 275 K-entry main dictionary,
  baked into `.rodata` (zero heap, zero load cost).
- Main dict flat view (used for `Dict::contains_main` only):
  ~2 MB (data) + ~1 MB (offsets) + ~40 KB (first-char index).
- Quantifier + stopword dicts: ~350 entries total.

Compare this to loading the Java `Dictionary` (nested
`HashMap<Character, DictSegment>`), which is several times larger on the
heap and pays GC + boxing costs on every lookup.

## Cargo features

| Feature      | Default | Effect                                   |
| ------------ | ------- | ---------------------------------------- |
| `main-dict`  | yes     | Bundle the ~275 K-entry main dictionary. |

Disable `default` to drop the main dictionary entirely (binary stays
~2 MB smaller). Quantifier + stopword dictionaries are always bundled —
they total only ~350 entries.

## License

Apache-2.0 — same as the upstream Java plugin.
