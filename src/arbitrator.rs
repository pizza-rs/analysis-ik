//! IK arbitrator — given a candidate lexeme set with overlaps, pick the best
//! non-overlapping subset (`ik_smart` mode). Ported from
//! `org.wltea.analyzer.core.IKArbitrator`.
//!
//! Scoring (each criterion is "more is better", evaluated left-to-right):
//! 1. **Number of chars covered** (more coverage wins).
//! 2. **Inverse lexeme count** (fewer lexemes wins).
//! 3. **Average lexeme length** (longer average wins).
//! 4. **Sum of kind priorities** (higher priority kinds win).

use crate::lexeme::{Lexeme, LexemeKind};

/// Pick a non-overlapping subset of lexemes that maximizes IK's smart-mode
/// scoring. `lexemes` may be in any order; the result is sorted by `begin`.
pub(crate) fn arbitrate(mut lexemes: Vec<Lexeme>, total_chars: usize) -> Vec<Lexeme> {
    if lexemes.is_empty() {
        return lexemes;
    }
    lexemes.sort_unstable_by_key(Lexeme::sort_key);
    lexemes.dedup();

    // Group lexemes into connected components (cross-paths). A new component
    // starts at the first lexeme whose `begin` >= max-end seen so far.
    let mut result: Vec<Lexeme> = Vec::with_capacity(lexemes.len());
    let mut i = 0;
    while i < lexemes.len() {
        let mut end = lexemes[i].end();
        let mut j = i + 1;
        while j < lexemes.len() && lexemes[j].begin < end {
            if lexemes[j].end() > end {
                end = lexemes[j].end();
            }
            j += 1;
        }
        // Component = lexemes[i..j]
        let component = &lexemes[i..j];
        if component.len() == 1 {
            result.push(component[0]);
        } else {
            let mut best: Vec<Lexeme> = Vec::new();
            let mut best_score = Score::WORST;
            let mut chosen: Vec<Lexeme> = Vec::new();
            dfs(component, 0, &mut chosen, &mut best, &mut best_score);
            result.extend(best);
        }
        i = j;
    }

    let _ = total_chars;
    result
}

#[derive(Debug, Clone, Copy)]
struct Score {
    covered: usize,
    neg_count: i64,
    avg_len_x_100: i64,
    priority_sum: i64,
}

impl Score {
    const WORST: Score = Score {
        covered: 0,
        neg_count: i64::MIN,
        avg_len_x_100: 0,
        priority_sum: 0,
    };

    fn better(&self, other: &Self) -> bool {
        if self.covered != other.covered {
            return self.covered > other.covered;
        }
        if self.neg_count != other.neg_count {
            return self.neg_count > other.neg_count;
        }
        if self.avg_len_x_100 != other.avg_len_x_100 {
            return self.avg_len_x_100 > other.avg_len_x_100;
        }
        self.priority_sum > other.priority_sum
    }
}

fn dfs(
    component: &[Lexeme],
    start_idx: usize,
    chosen: &mut Vec<Lexeme>,
    best: &mut Vec<Lexeme>,
    best_score: &mut Score,
) {
    // Skip option (also serves as the terminating branch).
    let cur_score = score(chosen);
    if cur_score.better(best_score) {
        *best_score = cur_score;
        *best = chosen.clone();
    }

    for k in start_idx..component.len() {
        let lex = component[k];
        // Compatible if it doesn't overlap with the last chosen lexeme.
        let compatible = chosen.last().map_or(true, |last| lex.begin >= last.end());
        if !compatible {
            continue;
        }
        chosen.push(lex);
        dfs(component, k + 1, chosen, best, best_score);
        chosen.pop();
    }
}

fn score(chosen: &[Lexeme]) -> Score {
    if chosen.is_empty() {
        return Score::WORST;
    }
    let covered: usize = chosen.iter().map(|l| l.length).sum();
    let count = chosen.len() as i64;
    let avg = (covered as i64 * 100) / count;
    let priority_sum: i64 = chosen.iter().map(|l| l.kind.priority() as i64).sum();
    Score {
        covered,
        neg_count: -count,
        avg_len_x_100: avg,
        priority_sum,
    }
}

/// Smart-mode post-processing: fuse adjacent `CnNum` + `Count` into `CnQuan`.
pub(crate) fn fuse_quantifiers(lexemes: &mut Vec<Lexeme>) {
    if lexemes.len() < 2 {
        return;
    }
    let mut out = Vec::with_capacity(lexemes.len());
    let mut i = 0;
    while i < lexemes.len() {
        if i + 1 < lexemes.len()
            && lexemes[i].kind == LexemeKind::CnNum
            && lexemes[i + 1].kind == LexemeKind::Count
            && lexemes[i].end() == lexemes[i + 1].begin
        {
            out.push(Lexeme {
                begin: lexemes[i].begin,
                length: lexemes[i].length + lexemes[i + 1].length,
                kind: LexemeKind::CnQuan,
            });
            i += 2;
        } else {
            out.push(lexemes[i]);
            i += 1;
        }
    }
    *lexemes = out;
}

/// MaxWord-mode counterpart of [`fuse_quantifiers`]: emit a `CnQuan` token
/// for every adjacent `CnNum` + `Count` pair WITHOUT removing the original
/// constituents. This is what keeps `Smart ⊆ MaxWord` for quantifier
/// phrases like "三个" or "五公斤".
///
/// `CnNum` and `Count` are only produced by the quantifier segmenter and
/// are typically a handful per input, so we scan the lexeme list twice
/// (once to collect `Count` positions, once to match `CnNum`) with no
/// auxiliary allocation beyond two small `Vec`s.
pub(crate) fn emit_quantifier_fusions(lexemes: &mut Vec<Lexeme>) {
    if lexemes.len() < 2 {
        return;
    }
    // Stack-friendly collection of (begin, length) for each kind. Most
    // inputs have ≤ a dozen of each.
    let mut nums: Vec<(usize, usize)> = Vec::new();
    let mut counts: Vec<(usize, usize)> = Vec::new();
    for lex in lexemes.iter() {
        match lex.kind {
            LexemeKind::CnNum => nums.push((lex.begin, lex.length)),
            LexemeKind::Count => counts.push((lex.begin, lex.length)),
            _ => {}
        }
    }
    if nums.is_empty() || counts.is_empty() {
        return;
    }
    // For each CnNum, find every Count that starts exactly where the
    // number ends and emit a CnQuan spanning both. `counts` is small so
    // the inner loop stays cheap.
    for &(nb, nl) in &nums {
        let join_at = nb + nl;
        for &(cb, cl) in &counts {
            if cb == join_at {
                lexemes.push(Lexeme {
                    begin: nb,
                    length: nl + cl,
                    kind: LexemeKind::CnQuan,
                });
            }
        }
    }
}

/// Emit any single CJK char that no other lexeme covers, as a `CnChar`
/// lexeme. Used in `ik_max_word` mode to ensure full coverage.
pub(crate) fn add_uncovered_chars(
    lexemes: &mut Vec<Lexeme>,
    kinds: &[crate::char_util::CharKind],
) {
    let n = kinds.len();
    let mut covered = vec![false; n];
    for lex in lexemes.iter() {
        for k in lex.begin..lex.end().min(n) {
            covered[k] = true;
        }
    }
    for i in 0..n {
        if !covered[i] && matches!(kinds[i], crate::char_util::CharKind::Chinese) {
            lexemes.push(Lexeme {
                begin: i,
                length: 1,
                kind: LexemeKind::CnChar,
            });
        }
    }
}

/// Counting-sort variant of `sort_unstable_by_key(Lexeme::sort_key) + dedup`.
/// Lexemes are first partitioned by `begin` (the high 32 bits of the sort
/// key), then each bucket is sorted by `(length desc, priority asc)`. This
/// is `O(N + K)` where `N == total_chars` and `K == lexemes.len()` —
/// noticeably faster than the comparison sort once `K` reaches the few
/// thousand lexemes typical of MaxWord on long Chinese text.
///
/// Lexemes from different `begin` values are never equal, so dedup only
/// needs to consider neighbours within a bucket.
pub(crate) fn bucket_sort_dedup_lexemes(lexemes: &mut Vec<Lexeme>, total_chars: usize) {
    let n = lexemes.len();
    if n < 2 {
        return;
    }

    // Counts per begin: `starts[i + 1]` will become the start of bucket i
    // after the prefix-sum pass.
    let mut starts: Vec<u32> = vec![0u32; total_chars + 2];
    for lex in lexemes.iter() {
        // Defensive clamp — segmenters never emit out-of-range begins, but
        // keep this function total.
        if lex.begin <= total_chars {
            starts[lex.begin + 1] += 1;
        }
    }
    for i in 1..starts.len() {
        starts[i] += starts[i - 1];
    }

    // Scatter using a per-bucket write cursor.
    let mut sorted: Vec<Lexeme> = Vec::with_capacity(n);
    // SAFETY: every slot is overwritten in the scatter loop below before
    // any read.
    unsafe {
        sorted.set_len(n);
    }
    let mut cursors: Vec<u32> = starts[..=total_chars].to_vec();
    for lex in lexemes.iter() {
        if lex.begin > total_chars {
            continue;
        }
        let slot = cursors[lex.begin] as usize;
        sorted[slot] = *lex;
        cursors[lex.begin] += 1;
    }

    // Per-bucket sort + dedup, compacting in place. `write <= lo` holds at
    // every iteration (deduped count ≤ raw count of all earlier buckets),
    // so the write head never clobbers data we still need to read.
    let mut write = 0usize;
    for p in 0..total_chars {
        let lo = starts[p] as usize;
        let hi = starts[p + 1] as usize;
        if lo == hi {
            continue;
        }
        if hi - lo > 1 {
            sorted[lo..hi].sort_unstable_by(|a, b| {
                b.length
                    .cmp(&a.length)
                    .then_with(|| a.kind.priority().cmp(&b.kind.priority()))
            });
        }
        let mut have_last = false;
        let mut last = Lexeme {
            begin: 0,
            length: 0,
            kind: LexemeKind::CnChar,
        };
        for j in lo..hi {
            let lex = sorted[j];
            if !have_last || lex != last {
                sorted[write] = lex;
                write += 1;
                last = lex;
                have_last = true;
            }
        }
    }
    sorted.truncate(write);
    *lexemes = sorted;
}
