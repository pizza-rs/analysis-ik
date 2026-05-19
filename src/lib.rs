//! # pizza-ik
//!
//! A from-scratch Rust port of the [analysis-ik](https://github.com/infinilabs/analysis-ik)
//! Java plugin. The runtime is zero-copy, allocation-light, and the bundled
//! dictionaries are compiled in via `build.rs` as compact binary blobs (no
//! filesystem access at runtime, no startup overhead).
//!
//! ## Quick start
//!
//! ```no_run
//! use pizza_engine::analysis::Tokenizer;
//! use pizza_ik::{IkConfig, IkMode, IkTokenizer};
//!
//! let tk = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart));
//! for tok in tk.tokenize("中华人民共和国成立于1949年") {
//!     println!("{} [{}..{}]", &*tok.term, tok.start_offset, tok.end_offset);
//! }
//! ```
//!
//! ## Two modes
//!
//! - [`IkMode::Smart`] — `ik_smart`. Picks a single best non-overlapping
//!   segmentation via a DP arbitrator (good for queries).
//! - [`IkMode::MaxWord`] — `ik_max_word`. Emits every dict hit, possibly
//!   overlapping (good for indexing — maximizes recall).
//!
//! ## Runtime extensions
//!
//! Add user words / stopwords without rebuilding:
//!
//! ```
//! use pizza_ik::{IkTokenizer, Rules};
//!
//! let mut rules = Rules::new();
//! rules.add_word("披萨").add_stopword("的");
//!
//! let tk = IkTokenizer::with_defaults().with_rules(rules);
//! # let _ = tk;
//! ```

#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]

mod arbitrator;
mod char_util;
mod config;
mod dict;
mod lexeme;
mod rules;
mod segmenter;
mod tokenizer;

pub use config::{IkConfig, IkMode};
pub use dict::{Dict, DictHit, DictKind};
pub use lexeme::{Lexeme, LexemeKind};
pub use rules::Rules;
pub use tokenizer::IkTokenizer;
