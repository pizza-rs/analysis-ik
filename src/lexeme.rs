//! Lexeme — the IK term-candidate. A lexeme covers a half-open char range
//! `[begin, begin+length)` and is tagged with its source ([`LexemeKind`]).

use core::cmp::Ordering;

/// Source / type of a lexeme. Mirrors `Lexeme.TYPE_*` constants in Java IK.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexemeKind {
    /// All-English letter run (e.g. `iPhone`).
    English,
    /// All-arabic digit run (e.g. `2024`).
    Arabic,
    /// Mixed alphanumeric / connector run (e.g. `iPhone-14`).
    Letter,
    /// Main-dict CJK word, ≥ 2 chars.
    CnWord,
    /// Single CJK char that isn't part of any longer dict word.
    CnChar,
    /// Hiragana/Katakana/Hangul/etc. run.
    OtherCjk,
    /// Chinese numeral run.
    CnNum,
    /// Quantifier-dict word.
    Count,
    /// Fused number + quantifier (e.g. `三个`).
    CnQuan,
}

impl LexemeKind {
    /// Mirrors Java IK's `getLexemeType` ordering used by the arbitrator.
    pub fn priority(self) -> u8 {
        match self {
            LexemeKind::English => 1,
            LexemeKind::Arabic => 2,
            LexemeKind::Letter => 3,
            LexemeKind::CnWord => 4,
            LexemeKind::OtherCjk => 8,
            LexemeKind::CnNum => 16,
            LexemeKind::Count => 32,
            LexemeKind::CnQuan => 48,
            LexemeKind::CnChar => 64,
        }
    }
}

/// One token candidate over the input character stream.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct Lexeme {
    /// Inclusive start char index.
    pub begin: usize,
    /// Length in chars (must be ≥ 1).
    pub length: usize,
    /// What kind of lexeme this is.
    pub kind: LexemeKind,
}

impl Lexeme {
    #[inline]
    pub fn end(&self) -> usize {
        self.begin + self.length
    }

    /// A packed `u64` whose unsigned natural order is identical to
    /// [`Lexeme::cmp`]: ascending by `begin`, descending by `length`, then
    /// ascending by kind priority. Used as a hot-path sort key —
    /// `sort_unstable_by_key(|l| l.sort_key())` reduces three field
    /// comparisons to a single 64-bit compare, which is roughly 2× faster
    /// on dense MaxWord output.
    ///
    /// Layout (high → low bits):
    ///   - 32 bits: `begin` (input is at most a few hundred thousand chars)
    ///   - 24 bits: `0xFF_FFFF - length` (so longer sorts before shorter)
    ///   -  8 bits: kind priority
    #[inline]
    pub fn sort_key(&self) -> u64 {
        let begin = (self.begin as u64) & 0xFFFF_FFFF;
        let len_inv = (0xFF_FFFFu64).saturating_sub(self.length as u64);
        let prio = self.kind.priority() as u64;
        (begin << 32) | (len_inv << 8) | prio
    }
}

impl Ord for Lexeme {
    /// Java IK's `QuickSortSet` ordering: ascending by begin, then **descending**
    /// by length (longer covers shorter first). Kind is the final tiebreaker
    /// so dedup behaves deterministically.
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

impl PartialOrd for Lexeme {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
