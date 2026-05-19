//! The public [`IkTokenizer`] implementing `pizza_engine::analysis::Tokenizer`.

use std::borrow::Cow;

use pizza_engine::analysis::{Token, Tokenizer};

use crate::arbitrator::{
    add_uncovered_chars, arbitrate, bucket_sort_dedup_lexemes, emit_quantifier_fusions,
    fuse_quantifiers,
};
use crate::char_util::CharKind;
use crate::config::{IkConfig, IkMode};
use crate::dict::{main_trie, stopword_view, view_contains, MainTrie};
use crate::lexeme::{Lexeme, LexemeKind};
use crate::rules::Rules;
use crate::segmenter::{
    cjk_segment, letter_segment, quantifier_segment, utf8_char_count, CharStream,
};

/// The IK tokenizer. Cheap to clone; safe to share across threads (Send + Sync).
#[derive(Debug, Clone, Default)]
pub struct IkTokenizer {
    pub(crate) config: IkConfig,
    pub(crate) rules: Rules,
}

impl IkTokenizer {
    /// Construct with the given [`IkConfig`] and an empty [`Rules`] overlay.
    pub fn new(config: IkConfig) -> Self {
        Self {
            config,
            rules: Rules::new(),
        }
    }

    /// Construct with default configuration (max-word, lowercase, no stopwords).
    pub fn with_defaults() -> Self {
        Self::default()
    }

    /// Chain-style: attach custom rules.
    pub fn with_rules(mut self, rules: Rules) -> Self {
        self.rules = rules;
        self
    }

    /// Mutably borrow the rules overlay, e.g. to add more words.
    pub fn rules_mut(&mut self) -> &mut Rules {
        &mut self.rules
    }

    /// Read-only access to the configuration.
    pub fn config(&self) -> &IkConfig {
        &self.config
    }
}

impl Tokenizer for IkTokenizer {
    fn tokenize<'a>(&self, text: &'a str) -> Vec<Token<'a>> {
        if text.is_empty() {
            return Vec::new();
        }
        let stream = CharStream::new(text, self.config.lowercase);
        let total = stream.chars.len();
        if total == 0 {
            return Vec::new();
        }
        match self.config.mode {
            IkMode::MaxWord => self.tokenize_max_word(text, &stream, total),
            IkMode::Smart => self.tokenize_smart(text, &stream, total),
        }
    }
}

impl IkTokenizer {
    /// MaxWord — **streaming** per-position emission. We avoid materializing
    /// the global `Vec<Lexeme>` and the `O(K log K)` sort that used to
    /// dominate this path. Letter / quantifier hits are precomputed once
    /// (they're sparse) and merged into a tiny per-position bucket alongside
    /// the trie-walked CJK hits; the bucket is sorted in place and tokens
    /// are pushed directly into the output.
    fn tokenize_max_word<'a>(
        &self,
        text: &'a str,
        stream: &CharStream<'a>,
        total: usize,
    ) -> Vec<Token<'a>> {
        let trie: MainTrie = main_trie();
        let ensure_subset = self.config.ensure_smart_subset;

        // Precompute non-CJK lexemes: letter / digit runs + quantifier hits,
        // plus the quantifier-fusion `CnQuan` tokens when the subset
        // guarantee is on. These are sparse in Chinese text, so a one-shot
        // sort by sort_key is cheap.
        let mut aux: Vec<Lexeme> = Vec::new();
        letter_segment(stream, &mut aux);
        quantifier_segment(stream, &mut aux);
        if ensure_subset {
            emit_quantifier_fusions(&mut aux);
        }
        aux.sort_unstable_by_key(Lexeme::sort_key);
        aux.dedup();

        let text_bytes = stream.text.as_bytes();
        let kinds = stream.kinds.as_slice();
        let byte_off = stream.byte_off.as_slice();
        let chars = stream.chars.as_slice();
        let needs_reg_check = stream.any_regularized;
        let stopwords = self.config.use_stopwords.then(stopword_view);
        // Hoist empty-set checks: the overwhelmingly common case is no
        // user-added extras and no removed bundled words, in which case we
        // can skip the per-position extra-prefix walk and the per-trie-hit
        // `is_removed_main` term-slice construction entirely.
        let has_removed = self.rules.has_removed_main();
        let has_extras = self.rules.has_extra_words();

        let mut tokens: Vec<Token<'a>> = Vec::with_capacity(total.saturating_mul(2));
        let mut position: u32 = 0;
        let mut last_begin: Option<usize> = None;
        // One scratch bucket reused across every position — no per-position
        // allocation. Typical bucket size is 1–8 entries.
        let mut bucket: Vec<Lexeme> = Vec::with_capacity(16);
        let mut aux_idx = 0usize;

        // Inline-dedup push: a linear scan over a tiny buffer is faster
        // than `sort_unstable_by + dedup` at this size and avoids the
        // per-position sort overhead entirely.
        #[inline(always)]
        fn push_unique(bucket: &mut Vec<Lexeme>, lex: Lexeme) {
            for existing in bucket.iter() {
                if *existing == lex {
                    return;
                }
            }
            bucket.push(lex);
        }

        for p in 0..total {
            bucket.clear();

            // CJK hits at p: walk the trie over the pre-decoded char slice
            // (no UTF-8 re-decoding), then user-extras (skipped entirely
            // when empty), then the optional forced singleton.
            if matches!(kinds[p], CharKind::Chinese) {
                let byte_start = byte_off[p];
                trie.find_prefixes_chars(&chars[p..], |char_count| {
                    // Only materialize the term slice when we actually need
                    // it for the removed-words check — saves a bounds-
                    // checked subslice on every hit in the common case.
                    if has_removed {
                        let byte_end = byte_off[p + char_count];
                        // SAFETY: text_bytes is valid UTF-8; byte_off
                        // values are codepoint boundaries.
                        let term = unsafe {
                            std::str::from_utf8_unchecked(&text_bytes[byte_start..byte_end])
                        };
                        if self.rules.is_removed_main(term) {
                            return;
                        }
                    }
                    let kind = if char_count == 1 {
                        LexemeKind::CnChar
                    } else {
                        LexemeKind::CnWord
                    };
                    push_unique(
                        &mut bucket,
                        Lexeme {
                            begin: p,
                            length: char_count,
                            kind,
                        },
                    );
                });
                if has_extras {
                    // SAFETY: same as above — byte_start is a codepoint
                    // boundary, text_bytes is valid UTF-8.
                    let tail_str = unsafe {
                        std::str::from_utf8_unchecked(&text_bytes[byte_start..])
                    };
                    self.rules.for_each_extra_prefix(tail_str, |term| {
                        let lc = utf8_char_count(term.as_bytes());
                        if lc == 0 {
                            return;
                        }
                        let kind = if lc == 1 {
                            LexemeKind::CnChar
                        } else {
                            LexemeKind::CnWord
                        };
                        push_unique(
                            &mut bucket,
                            Lexeme {
                                begin: p,
                                length: lc,
                                kind,
                            },
                        );
                    });
                }
                if ensure_subset {
                    push_unique(
                        &mut bucket,
                        Lexeme {
                            begin: p,
                            length: 1,
                            kind: LexemeKind::CnChar,
                        },
                    );
                }
            }

            // Drain precomputed letter / quantifier / fusion entries that
            // start exactly at p (aux is sorted by sort_key — begin asc).
            while aux_idx < aux.len() && aux[aux_idx].begin == p {
                push_unique(&mut bucket, aux[aux_idx]);
                aux_idx += 1;
            }

            if bucket.is_empty() {
                continue;
            }

            // Emit tokens for this position. `position` is incremented once
            // per distinct begin that actually produces an output token
            // (stopword-filtered entries don't bump position).
            let mut assigned_for_p = false;
            for lex in bucket.iter() {
                let bs = byte_off[lex.begin];
                let be = byte_off[lex.begin + lex.length];
                let term: Cow<'a, str> = if !needs_reg_check {
                    Cow::Borrowed(&text[bs..be])
                } else {
                    let reg_slice = &chars[lex.begin..lex.begin + lex.length];
                    if regularization_is_noop(&text[bs..be], reg_slice) {
                        Cow::Borrowed(&text[bs..be])
                    } else {
                        let mut s = String::with_capacity(be - bs);
                        for &c in reg_slice {
                            s.push(c);
                        }
                        Cow::Owned(s)
                    }
                };

                if let Some(stop_view) = stopwords.as_ref() {
                    let s: &str = &term;
                    let is_sw = !self.rules.is_removed_stopword(s)
                        && (self.rules.is_extra_stopword(s) || view_contains(stop_view, s));
                    if is_sw {
                        continue;
                    }
                }

                if !assigned_for_p {
                    if last_begin.is_some() {
                        position += 1;
                    }
                    last_begin = Some(p);
                    assigned_for_p = true;
                }

                tokens.push(Token {
                    term,
                    start_offset: bs as u32,
                    end_offset: be as u32,
                    position,
                });
            }
        }

        tokens
    }

    /// Smart — runs all three segmenters, arbitrates, fuses quantifiers,
    /// then bucket-sorts the final lexeme set in `O(N + K)` instead of the
    /// previous `O(K log K)` comparison sort.
    fn tokenize_smart<'a>(
        &self,
        text: &'a str,
        stream: &CharStream<'a>,
        total: usize,
    ) -> Vec<Token<'a>> {
        let mut lexemes: Vec<Lexeme> = Vec::with_capacity(total);
        cjk_segment(stream, &self.rules, &mut lexemes);
        letter_segment(stream, &mut lexemes);
        quantifier_segment(stream, &mut lexemes);

        let mut final_lexemes = arbitrate(lexemes, total);
        fuse_quantifiers(&mut final_lexemes);
        add_uncovered_chars(&mut final_lexemes, &stream.kinds);
        ensure_isolated_alnum(&mut final_lexemes, &stream.kinds);
        bucket_sort_dedup_lexemes(&mut final_lexemes, total);

        let stopwords = self.config.use_stopwords.then(stopword_view);
        let mut tokens: Vec<Token<'a>> = Vec::with_capacity(final_lexemes.len());
        let mut position: u32 = 0;
        let mut last_begin: Option<usize> = None;
        let needs_reg_check = stream.any_regularized;
        for lex in final_lexemes.iter() {
            let byte_start = stream.byte_off[lex.begin];
            let byte_end = stream.byte_off[lex.begin + lex.length];
            let term: Cow<'a, str> = if !needs_reg_check {
                Cow::Borrowed(&text[byte_start..byte_end])
            } else {
                let reg_slice = &stream.chars[lex.begin..lex.begin + lex.length];
                if regularization_is_noop(&text[byte_start..byte_end], reg_slice) {
                    Cow::Borrowed(&text[byte_start..byte_end])
                } else {
                    let mut s = String::with_capacity(byte_end - byte_start);
                    for &c in reg_slice {
                        s.push(c);
                    }
                    Cow::Owned(s)
                }
            };

            if let Some(stopwords) = stopwords.as_ref() {
                let s: &str = &term;
                let is_sw = !self.rules.is_removed_stopword(s)
                    && (self.rules.is_extra_stopword(s) || view_contains(stopwords, s));
                if is_sw {
                    continue;
                }
            }

            if Some(lex.begin) != last_begin {
                if last_begin.is_some() {
                    position += 1;
                }
                last_begin = Some(lex.begin);
            }

            tokens.push(Token {
                term,
                start_offset: byte_start as u32,
                end_offset: byte_end as u32,
                position,
            });
        }

        tokens
    }
}

fn ensure_isolated_alnum(lexemes: &mut Vec<Lexeme>, kinds: &[CharKind]) {
    let n = kinds.len();
    let mut covered = vec![false; n];
    for lex in lexemes.iter() {
        for k in lex.begin..lex.end().min(n) {
            covered[k] = true;
        }
    }
    for i in 0..n {
        if covered[i] {
            continue;
        }
        match kinds[i] {
            CharKind::English => lexemes.push(Lexeme {
                begin: i,
                length: 1,
                kind: LexemeKind::English,
            }),
            CharKind::Arabic => lexemes.push(Lexeme {
                begin: i,
                length: 1,
                kind: LexemeKind::Arabic,
            }),
            CharKind::OtherCjk => lexemes.push(Lexeme {
                begin: i,
                length: 1,
                kind: LexemeKind::OtherCjk,
            }),
            _ => {}
        }
    }
}

/// Append a `CnChar` lexeme at every Chinese position regardless of
/// existing coverage. Used in MaxWord mode so the index contains every
/// single-char token a Smart-mode query might emit at that position after
/// arbitration drops the longer covering lexeme. Duplicates are removed by
/// the caller's `sort_unstable` + `dedup`.
fn force_emit_cjk_singletons(lexemes: &mut Vec<Lexeme>, kinds: &[CharKind]) {
    // No longer used on the hot path — the streaming MaxWord tokenizer
    // bakes this in directly. Kept (and `#[allow(dead_code)]`-tagged) only
    // so external callers / tests that referenced this helper still
    // compile cleanly during the transition.
    #![allow(dead_code)]
    for (i, k) in kinds.iter().enumerate() {
        if matches!(k, CharKind::Chinese) {
            lexemes.push(Lexeme {
                begin: i,
                length: 1,
                kind: LexemeKind::CnChar,
            });
        }
    }
}

#[inline]
fn regularization_is_noop(original: &str, regularized: &[char]) -> bool {
    let mut iter = original.chars();
    for &rc in regularized {
        match iter.next() {
            Some(oc) if oc == rc => continue,
            _ => return false,
        }
    }
    iter.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{IkConfig, IkMode};
    use std::collections::HashSet;

    fn token_keys(t: &IkTokenizer, text: &str) -> HashSet<(String, u32, u32)> {
        t.tokenize(text)
            .into_iter()
            .map(|tok| (tok.term.into_owned(), tok.start_offset, tok.end_offset))
            .collect()
    }

    /// Invariant: tokens produced by `IkMode::Smart` are always a subset of
    /// tokens produced by `IkMode::MaxWord` (compared as
    /// `(term, start_offset, end_offset)`). This is the contract that lets
    /// callers index with MaxWord and query with Smart while guaranteeing
    /// every Smart-emitted query token exists in the index.
    #[test]
    fn smart_is_subset_of_maxword() {
        let cases = [
            "三个苹果",
            "我买了三个苹果和五公斤大米",
            "起源于古代神话",
            "南京市长江大桥",
            "iPhone 14在中国大陆销售三个版本",
            "一千零一夜的故事讲了好多遍",
            "中华人民共和国成立于1949年。北京大学生喝进口红酒。",
        ];
        let smart = IkTokenizer::new(IkConfig::for_searching().mode(IkMode::Smart));
        let max_word = IkTokenizer::new(IkConfig::for_indexing().mode(IkMode::MaxWord));
        for text in cases {
            let s = token_keys(&smart, text);
            let m = token_keys(&max_word, text);
            let missing: Vec<_> = s.difference(&m).collect();
            assert!(
                missing.is_empty(),
                "Smart leaked tokens not in MaxWord for {text:?}: {missing:?}"
            );
        }
    }
}
