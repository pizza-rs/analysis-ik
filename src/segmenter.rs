//! The three IK sub-segmenters, ported from
//! `org.wltea.analyzer.core.CJKSegmenter`, `LetterSegmenter`, and
//! `CN_QuantifierSegmenter`.

use crate::char_util::{classify, is_chinese_number, regularize, CharKind};
use crate::dict::{for_each_prefix_in, main_trie, quantifier_view};
use crate::lexeme::{Lexeme, LexemeKind};
use crate::rules::Rules;

/// Pre-computed view of the input: the regularized char sequence and the
/// byte offset of every char in the original UTF-8 string. `byte_off` has
/// length `chars.len() + 1` so we can do `text[byte_off[a]..byte_off[b]]`.
pub(crate) struct CharStream<'a> {
    pub chars: Vec<char>,
    pub byte_off: Vec<usize>,
    pub kinds: Vec<CharKind>,
    pub text: &'a str,
    /// `true` when at least one input char was changed by `regularize` (ASCII
    /// uppercase → lowercase, or full-width → half-width). When `false`, the
    /// token emission path can skip the per-token `regularization_is_noop`
    /// check and always borrow directly from `text`.
    pub any_regularized: bool,
}

impl<'a> CharStream<'a> {
    /// Build the stream in a single pass, fusing regularization, byte-offset
    /// recording, and char-kind classification. Avoids the intermediate
    /// `Vec<(char, usize)>` that the previous two-pass version allocated.
    pub fn new(text: &'a str, lowercase: bool) -> Self {
        // Heuristic capacity. Pure-CJK input has ~3 bytes/char; ASCII has 1.
        let cap = text.len() / 2 + 1;
        let mut chars = Vec::with_capacity(cap);
        let mut byte_off = Vec::with_capacity(cap + 1);
        let mut kinds = Vec::with_capacity(cap);
        let mut any_regularized = false;
        for (off, c) in text.char_indices() {
            let r = regularize(c, lowercase);
            if r != c {
                any_regularized = true;
            }
            chars.push(r);
            byte_off.push(off);
            kinds.push(classify(r));
        }
        byte_off.push(text.len());
        Self {
            chars,
            byte_off,
            kinds,
            text,
            any_regularized,
        }
    }
}

/// Count UTF-8 codepoints in `bytes` without allocating: a leading byte is
/// any byte whose top two bits are not `10` (continuation bytes are `10xxxxxx`).
#[inline]
pub(crate) fn utf8_char_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| (b & 0xC0) != 0x80).count()
}

// ---------------------------------------------------------------------------
// CJK segmenter (main-dict trie hits, plus user extras)
// ---------------------------------------------------------------------------

pub(crate) fn cjk_segment(stream: &CharStream<'_>, rules: &Rules, out: &mut Vec<Lexeme>) {
    let trie = main_trie();
    let text_bytes = stream.text.as_bytes();
    // For each Chinese position, find every main-dict term whose first char
    // matches and which is a prefix of the remaining text. Real CJK
    // characters are unaffected by `regularize`, so we can compare against
    // the original UTF-8 bytes directly — no allocation needed.
    let chars = stream.chars.as_slice();
    let byte_off = stream.byte_off.as_slice();
    let has_removed = rules.has_removed_main();
    let has_extras = rules.has_extra_words();
    for start in 0..chars.len() {
        if !matches!(stream.kinds[start], CharKind::Chinese) {
            continue;
        }
        let byte_start = byte_off[start];

        // Walk the compile-time trie over the pre-decoded char slice — no
        // per-position UTF-8 re-decoding. Each step is O(log fanout); the
        // callback fires once per terminal node hit, in length-ascending
        // order.
        trie.find_prefixes_chars(&chars[start..], |char_count| {
            if has_removed {
                let byte_end = byte_off[start + char_count];
                // SAFETY: text_bytes is valid UTF-8; byte_off values are
                // codepoint boundaries.
                let term = unsafe {
                    std::str::from_utf8_unchecked(&text_bytes[byte_start..byte_end])
                };
                if rules.is_removed_main(term) {
                    return;
                }
            }
            let kind = if char_count == 1 {
                LexemeKind::CnChar
            } else {
                LexemeKind::CnWord
            };
            out.push(Lexeme {
                begin: start,
                length: char_count,
                kind,
            });
        });

        // User extras. Skipped entirely (no `tail_str` construction, no
        // `BTreeSet::range` walk) when the user added none — the common
        // case.
        if has_extras {
            let tail_str = unsafe {
                std::str::from_utf8_unchecked(&text_bytes[byte_start..])
            };
            rules.for_each_extra_prefix(tail_str, |term| {
                let lc = utf8_char_count(term.as_bytes());
                if lc == 0 {
                    return;
                }
                let kind = if lc == 1 {
                    LexemeKind::CnChar
                } else {
                    LexemeKind::CnWord
                };
                out.push(Lexeme {
                    begin: start,
                    length: lc,
                    kind,
                });
            });
        }
    }
}

// ---------------------------------------------------------------------------
// Letter / number / mixed segmenter
// ---------------------------------------------------------------------------

const ENG_CONNECTORS: &[char] = &['#', '&', '+', '-', '.', '@', '_'];
const NUM_CONNECTORS: &[char] = &[',', '.'];

pub(crate) fn letter_segment(stream: &CharStream<'_>, out: &mut Vec<Lexeme>) {
    let chars = &stream.chars;
    let kinds = &stream.kinds;
    let n = chars.len();

    // English-only run.
    let mut i = 0;
    while i < n {
        if matches!(kinds[i], CharKind::English) {
            let start = i;
            let mut end = i + 1;
            // Greedy extend: letters, or connectors followed by another letter.
            while end < n {
                let c = chars[end];
                if matches!(kinds[end], CharKind::English) {
                    end += 1;
                } else if ENG_CONNECTORS.contains(&c) && end + 1 < n
                    && matches!(kinds[end + 1], CharKind::English)
                {
                    end += 2;
                } else {
                    break;
                }
            }
            out.push(Lexeme {
                begin: start,
                length: end - start,
                kind: LexemeKind::English,
            });
            i = end;
        } else {
            i += 1;
        }
    }

    // Arabic-only run.
    let mut i = 0;
    while i < n {
        if matches!(kinds[i], CharKind::Arabic) {
            let start = i;
            let mut end = i + 1;
            while end < n {
                let c = chars[end];
                if matches!(kinds[end], CharKind::Arabic) {
                    end += 1;
                } else if NUM_CONNECTORS.contains(&c) && end + 1 < n
                    && matches!(kinds[end + 1], CharKind::Arabic)
                {
                    end += 2;
                } else {
                    break;
                }
            }
            out.push(Lexeme {
                begin: start,
                length: end - start,
                kind: LexemeKind::Arabic,
            });
            i = end;
        } else {
            i += 1;
        }
    }

    // Mixed letter+digit run (with all connectors). Must start with a letter
    // or digit, must include at least one connector or transition.
    let mut i = 0;
    while i < n {
        let k = kinds[i];
        if !matches!(k, CharKind::English | CharKind::Arabic) {
            i += 1;
            continue;
        }
        let start = i;
        let mut end = i + 1;
        let mut saw_alnum = true; // current char
        while end < n {
            let c = chars[end];
            let ck = kinds[end];
            if matches!(ck, CharKind::English | CharKind::Arabic) {
                end += 1;
                saw_alnum = true;
            } else if (ENG_CONNECTORS.contains(&c) || NUM_CONNECTORS.contains(&c))
                && end + 1 < n
                && matches!(kinds[end + 1], CharKind::English | CharKind::Arabic)
            {
                end += 2;
                saw_alnum = true;
            } else {
                break;
            }
        }
        // Only emit if it's longer than what English / Arabic runs alone would
        // produce (i.e. it actually mixes types or includes a connector).
        if end - start > 1 && saw_alnum {
            let mut has_letter = false;
            let mut has_digit = false;
            let mut has_connector = false;
            for &c in &chars[start..end] {
                if c.is_ascii_alphabetic() {
                    has_letter = true;
                } else if c.is_ascii_digit() {
                    has_digit = true;
                } else {
                    has_connector = true;
                }
            }
            if (has_letter && has_digit) || has_connector {
                out.push(Lexeme {
                    begin: start,
                    length: end - start,
                    kind: LexemeKind::Letter,
                });
            }
        }
        i = end;
    }
}

// ---------------------------------------------------------------------------
// Chinese number / quantifier segmenter
// ---------------------------------------------------------------------------

pub(crate) fn quantifier_segment(stream: &CharStream<'_>, out: &mut Vec<Lexeme>) {
    let chars = &stream.chars;
    let n = chars.len();
    let qview = quantifier_view();

    // CnNum runs.
    let mut i = 0;
    while i < n {
        if is_chinese_number(chars[i]) {
            let start = i;
            let mut end = i + 1;
            while end < n && is_chinese_number(chars[end]) {
                end += 1;
            }
            out.push(Lexeme {
                begin: start,
                length: end - start,
                kind: LexemeKind::CnNum,
            });
            i = end;
        } else {
            i += 1;
        }
    }

    // Quantifier dict hits — fire only where the previous run was a CnNum
    // (Java IK: only emits Count when number context is active).
    let mut number_until = 0usize; // exclusive end of last number run
    let mut p = 0usize;
    while p < n {
        if is_chinese_number(chars[p]) {
            let start = p;
            while p < n && is_chinese_number(chars[p]) {
                p += 1;
            }
            number_until = p;
            let _ = start;
            continue;
        }
        // We're at a non-number position; only emit quantifier hits if we
        // just came out of a number run, OR if the previous char was a digit.
        let active = p == number_until
            || (p > 0 && matches!(stream.kinds[p - 1], CharKind::Arabic));
        if active && matches!(stream.kinds[p], CharKind::Chinese) {
            let byte_start = stream.byte_off[p];
            let tail_bytes = &stream.text.as_bytes()[byte_start..];
            for_each_prefix_in(&qview, tail_bytes, |term| {
                let lc = utf8_char_count(term);
                if lc == 0 {
                    return;
                }
                out.push(Lexeme {
                    begin: p,
                    length: lc,
                    kind: LexemeKind::Count,
                });
            });
        }
        p += 1;
    }
}
