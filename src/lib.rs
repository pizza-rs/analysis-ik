//! # pizza-ik
//!
//! A from-scratch Rust port of the [analysis-ik](https://github.com/pizza-rs/analysis-ik)
//! Java plugin. The runtime is zero-copy, allocation-light, and the bundled
//! dictionaries are compiled in via `build.rs` as compact binary blobs (no
//! filesystem access at runtime, no startup overhead).
//!
//! ## Quick start
//!
//! ```no_run
//! use pizza_analysis_ik::IkConfig;
//! use pizza_analysis_ik::IkMode;
//! use pizza_analysis_ik::IkTokenizer;
//! use pizza_engine::analysis::Tokenizer;
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
//! use pizza_analysis_ik::IkTokenizer;
//! use pizza_analysis_ik::Rules;
//!
//! let mut rules = Rules::new();
//! rules.add_word("披萨").add_stopword("的");
//!
//! let tk = IkTokenizer::with_defaults().with_rules(rules);
//! # let _ = tk;
//! ```

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(rust_2018_idioms)]
#![warn(missing_debug_implementations)]

extern crate alloc;
mod arbitrator;
mod char_util;
mod config;
mod dict;
mod lexeme;
mod rules;
mod segmenter;
mod tokenizer;

pub use config::IkConfig;
pub use config::IkMode;
pub use dict::Dict;
pub use dict::DictHit;
pub use dict::DictKind;
pub use lexeme::Lexeme;
pub use lexeme::LexemeKind;
pub use rules::Rules;
pub use tokenizer::IkTokenizer;
pub use tokenizer::TokenRange;
pub mod register;
pub use register::register_all;
