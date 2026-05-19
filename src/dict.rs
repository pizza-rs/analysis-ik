//! Bundled IK dictionaries.
//!
//! Each dict is a compile-time-baked, sorted list of UTF-8 terms with a
//! first-character bucket index. Lookups never allocate and never touch the
//! filesystem.
//!
//! - Term storage: a single `&'static [u8]` blob + a parallel `&'static [u8]`
//!   offsets blob (u32 LE values), so very large dictionaries don't bloat
//!   the generated Rust source.
//! - First-char index: a small sorted `&[u32]` for `O(log B)` bucket lookup,
//!   where `B` is the number of distinct first characters (≈ 5K for
//!   `main.dic`).

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

/// Source of a hit returned from [`Dict::longest_or_all`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictKind {
    Main,
    Quantifier,
    Stopword,
    /// User-supplied extension (see [`crate::Rules`]).
    Extra,
}

/// A single in-dictionary hit: a `[start, end)` range over the **char**
/// indices of the input, together with the source dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DictHit {
    pub start: usize,
    pub end: usize,
    pub kind: DictKind,
}

/// Read the `i`-th little-endian `u32` from `buf`. Uses one unaligned
/// load (a single `mov` on x86, `ldur` on AArch64) and a debug-only
/// bounds check, instead of an array-literal + `from_le_bytes` which the
/// optimizer doesn't always fold cleanly.
#[inline(always)]
fn read_u32_le(buf: &[u8], i: usize) -> u32 {
    let s = i * 4;
    debug_assert!(s + 4 <= buf.len(), "read_u32_le out of bounds");
    // SAFETY: caller-controlled index; debug_assert above and a release-mode
    // bounds check on the originating slice (e.g. trie root walk only ever
    // dereferences `node` values produced by the same trie's own edges)
    // keep this in range.
    unsafe {
        let ptr = buf.as_ptr().add(s) as *const u32;
        u32::from_le(core::ptr::read_unaligned(ptr))
    }
}

/// A single bundled (or user-supplied) dictionary. All lookups go through
/// this view; the underlying storage is `&'static` for the bundled dicts
/// and `&'a` for user extras.
#[derive(Clone, Copy)]
pub(crate) struct DictView<'a> {
    pub fc: &'a [u32],
    pub bucket_start: &'a [u32], // len = fc.len() + 1
    pub data: &'a [u8],
    pub offsets_bin: &'a [u8],
    pub count: usize,
}

impl<'a> DictView<'a> {
    #[inline]
    fn term_bytes(&self, idx: u32) -> &'a [u8] {
        let s = read_u32_le(self.offsets_bin, idx as usize) as usize;
        let e = read_u32_le(self.offsets_bin, idx as usize + 1) as usize;
        &self.data[s..e]
    }

    /// Locate the bucket index for first char `c`. Returns the bucket index
    /// into `self.fc`, or `None`.
    #[inline]
    fn bucket_of(&self, c: char) -> Option<usize> {
        let key = c as u32;
        self.fc.binary_search(&key).ok()
    }

    /// Find every term in the dictionary that is a **prefix** of `text`.
    /// Hits are emitted in order of increasing byte length.
    ///
    /// Strategy: locate the first-char bucket via a single binary search,
    /// then for each candidate prefix length do **one** binary search
    /// inside a *cursor-narrowed* sub-range of the bucket. The cursor
    /// monotonically advances as the candidate grows, so deeper-prefix
    /// searches inspect progressively fewer entries (typically 1–4 per
    /// step after the first). Net cost per starting position is
    /// `O(log |bucket|) + O(L)` rather than the previous `O(L · log |bucket|)`
    /// with a doubled coefficient from a separate "any-starts-with" probe.
    fn find_prefixes<F: FnMut(&'a [u8])>(&self, text: &'a [u8], mut f: F) {
        if text.is_empty() {
            return;
        }
        let first = match std::str::from_utf8(text).ok().and_then(|s| s.chars().next()) {
            Some(c) => c,
            None => return,
        };
        let Some(bk) = self.bucket_of(first) else {
            return;
        };
        let lo = self.bucket_start[bk] as usize;
        let hi = self.bucket_start[bk + 1] as usize;

        let mut byte_end = 0usize;
        let mut cursor = lo;
        for c in unsafe { std::str::from_utf8_unchecked(text) }.chars() {
            byte_end += c.len_utf8();
            let candidate = &text[..byte_end];

            // Binary search [cursor, hi) for first idx where bucket[idx] >= candidate.
            let mut left = cursor;
            let mut right = hi;
            while left < right {
                let mid = left + (right - left) / 2;
                if self.term_bytes(mid as u32) < candidate {
                    left = mid + 1;
                } else {
                    right = mid;
                }
            }
            if left >= hi {
                break;
            }
            let term = self.term_bytes(left as u32);
            if term == candidate {
                f(candidate);
            }
            // If the smallest entry >= candidate doesn't even start with
            // candidate, no longer prefix can match either: stop.
            if !term.starts_with(candidate) {
                break;
            }
            // Future candidates extend `candidate`, so they're all >= current
            // candidate. Advance the cursor — any entry < `left` is strictly
            // less than every longer candidate too.
            cursor = left;
        }
    }
}

#[inline]
fn partition_point_in_bucket(view: &DictView<'_>, lo: u32, len: usize, key: &[u8]) -> usize {
    // Binary search for first idx in [0, len) where bucket[idx] >= key.
    let mut left = 0usize;
    let mut right = len;
    while left < right {
        let mid = left + (right - left) / 2;
        let term = view.term_bytes(lo + mid as u32);
        if term < key {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    left
}

/// Read-only handle for the bundled `MAIN_*` dictionary.
#[inline]
pub(crate) fn main_view() -> DictView<'static> {
    DictView {
        fc: MAIN_FC,
        bucket_start: MAIN_BUCKET_START,
        data: MAIN_DATA,
        offsets_bin: MAIN_OFFSETS_BIN,
        count: MAIN_TERM_COUNT,
    }
}

// -----------------------------------------------------------------------------
// Main-dict trie (compile-time graph data structure)
// -----------------------------------------------------------------------------

/// Compact char-level prefix trie for the main dictionary, stored as three
/// `&'static [u8]` blobs (CSR layout). All slices are read-only and the
/// structure performs zero allocations.
///
/// Lookup cost: walking an `L`-char prefix costs `O(L · log fanout)` where
/// fanout is the number of distinct continuations at the current trie node
/// (typically 1–5 inside the dictionary, ~5K at the root). Compared with the
/// flat sorted-array dict view, this replaces the per-length `O(log B)`
/// binary search (B ≈ 55 average bucket size) with a `O(log fanout)` step.
#[derive(Clone, Copy)]
pub(crate) struct MainTrie {
    /// `(nodes + 1) × u32 LE`. Bit 31 = terminal flag; low 31 bits = first
    /// edge index. Sentinel at the end stores only the total edge count.
    nodes: &'static [u8],
    /// `edges × u32 LE` — codepoint label of each edge, sorted within a node.
    edge_char: &'static [u8],
    /// `edges × u32 LE` — destination node id.
    edge_next: &'static [u8],
    /// Root direct-lookup table: `(root_max - root_min + 1) × u32 LE`.
    /// `root_index[c - root_min]` is the destination node id for codepoint
    /// `c`, or `u32::MAX` for "no edge". Replaces the ~13-step binary
    /// search at the trie root with a single load on the hot path.
    root_index: &'static [u8],
    root_min: u32,
    root_max: u32,
}

impl MainTrie {
    #[inline]
    fn node_entry(&self, node: u32) -> u32 {
        read_u32_le(self.nodes, node as usize)
    }

    /// Returns `(terminal, first_edge, last_edge)` for `node`. The two
    /// adjacent `u32` reads are fused into one unaligned `u64` load so we
    /// only pay one cache miss per node visit and the inner loop reads
    /// one fewer cache line on hot paths.
    #[inline]
    fn node_info(&self, node: u32) -> (bool, u32, u32) {
        let s = node as usize * 4;
        debug_assert!(s + 8 <= self.nodes.len(), "node_info out of bounds");
        // SAFETY: same correctness invariant as `read_u32_le` — `node` only
        // comes from edge destinations within this trie, which are bounds-
        // checked at build time.
        let pair = unsafe {
            let ptr = self.nodes.as_ptr().add(s) as *const u64;
            u64::from_le(core::ptr::read_unaligned(ptr))
        };
        let cur = pair as u32;
        let nxt = (pair >> 32) as u32;
        let terminal = (cur & 0x8000_0000) != 0;
        let first = cur & 0x7FFF_FFFF;
        let last = nxt & 0x7FFF_FFFF;
        (terminal, first, last)
    }

    #[inline]
    fn edge_char_at(&self, idx: u32) -> u32 {
        read_u32_le(self.edge_char, idx as usize)
    }

    #[inline]
    fn edge_next_at(&self, idx: u32) -> u32 {
        read_u32_le(self.edge_next, idx as usize)
    }

    /// Single-load lookup of the root's child for codepoint `c`. Returns
    /// the destination node id, or `None` if the root has no edge for `c`.
    /// Replaces the ~13-step binary search at the trie root with one
    /// range check + one `u32` load.
    #[inline]
    fn root_child(&self, c: u32) -> Option<u32> {
        if c < self.root_min || c > self.root_max {
            return None;
        }
        let v = read_u32_le(self.root_index, (c - self.root_min) as usize);
        if v == u32::MAX {
            None
        } else {
            Some(v)
        }
    }

    /// For every dict term that is a prefix of `text`, invoke
    /// `f(byte_len, char_count)`. Both lengths refer to the matched prefix
    /// within `text`. Hits are emitted in order of increasing length.
    pub(crate) fn find_prefixes<F: FnMut(usize, usize)>(&self, text: &str, mut f: F) {
        let mut node: u32 = 0;
        let mut byte_pos: usize = 0;
        let mut char_count: usize = 0;
        for c in text.chars() {
            let (_, first, last) = self.node_info(node);
            if first == last {
                return;
            }
            // Binary search edges [first, last) for codepoint `c`.
            let key = c as u32;
            let mut lo = first;
            let mut hi = last;
            let mut found: Option<u32> = None;
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let ec = self.edge_char_at(mid);
                if ec < key {
                    lo = mid + 1;
                } else if ec > key {
                    hi = mid;
                } else {
                    found = Some(mid);
                    break;
                }
            }
            let Some(eidx) = found else {
                return;
            };
            node = self.edge_next_at(eidx);
            byte_pos += c.len_utf8();
            char_count += 1;
            // Terminal lookup is one u32 read.
            if (self.node_entry(node) & 0x8000_0000) != 0 {
                f(byte_pos, char_count);
            }
        }
    }

    /// Same as [`find_prefixes`] but consumes a pre-decoded `&[char]`
    /// slice. The streaming tokenizer already has `CharStream::chars`
    /// available, so this skips per-position UTF-8 re-decoding (one
    /// `chars()` walk per starting position) — a notable win for the
    /// MaxWord hot loop on long CJK input. The callback receives only the
    /// matched `char_count`; the caller already knows byte offsets via
    /// `CharStream::byte_off`.
    #[inline]
    pub(crate) fn find_prefixes_chars<F: FnMut(usize)>(&self, chars: &[char], mut f: F) {
        if chars.is_empty() {
            return;
        }
        // First step: O(1) root direct lookup, replacing the ~13-step
        // binary search over ~5K root children.
        let mut node: u32 = match self.root_child(chars[0] as u32) {
            Some(n) => n,
            None => return,
        };
        let mut char_count: usize = 1;
        if (self.node_entry(node) & 0x8000_0000) != 0 {
            f(char_count);
        }

        // Subsequent steps: typical inner-node fanout is 1–5, so binary
        // search remains the right choice.
        for &c in &chars[1..] {
            let (_, first, last) = self.node_info(node);
            if first == last {
                return;
            }
            let key = c as u32;
            let mut lo = first;
            let mut hi = last;
            let mut found: Option<u32> = None;
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let ec = self.edge_char_at(mid);
                if ec < key {
                    lo = mid + 1;
                } else if ec > key {
                    hi = mid;
                } else {
                    found = Some(mid);
                    break;
                }
            }
            let Some(eidx) = found else {
                return;
            };
            node = self.edge_next_at(eidx);
            char_count += 1;
            if (self.node_entry(node) & 0x8000_0000) != 0 {
                f(char_count);
            }
        }
    }

    /// Membership test against the trie. `O(L · log fanout)`.
    pub(crate) fn contains(&self, word: &str) -> bool {
        if word.is_empty() {
            return false;
        }
        let mut node: u32 = 0;
        for c in word.chars() {
            let (_, first, last) = self.node_info(node);
            if first == last {
                return false;
            }
            let key = c as u32;
            let mut lo = first;
            let mut hi = last;
            let mut found: Option<u32> = None;
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let ec = self.edge_char_at(mid);
                if ec < key {
                    lo = mid + 1;
                } else if ec > key {
                    hi = mid;
                } else {
                    found = Some(mid);
                    break;
                }
            }
            match found {
                Some(eidx) => node = self.edge_next_at(eidx),
                None => return false,
            }
        }
        (self.node_entry(node) & 0x8000_0000) != 0
    }
}

#[inline]
pub(crate) fn main_trie() -> MainTrie {
    MainTrie {
        nodes: MAIN_TRIE_NODES,
        edge_char: MAIN_TRIE_EDGE_CHAR,
        edge_next: MAIN_TRIE_EDGE_NEXT,
        root_index: MAIN_TRIE_ROOT_INDEX,
        root_min: MAIN_TRIE_ROOT_MIN,
        root_max: MAIN_TRIE_ROOT_MAX,
    }
}

/// Read-only handle for the bundled `QUANTIFIER_*` dictionary.
#[inline]
pub(crate) fn quantifier_view() -> DictView<'static> {
    DictView {
        fc: QUANTIFIER_FC,
        bucket_start: QUANTIFIER_BUCKET_START,
        data: QUANTIFIER_DATA,
        offsets_bin: QUANTIFIER_OFFSETS_BIN,
        count: QUANTIFIER_TERM_COUNT,
    }
}

/// Read-only handle for the bundled `STOPWORD_*` dictionary.
#[inline]
pub(crate) fn stopword_view() -> DictView<'static> {
    DictView {
        fc: STOPWORD_FC,
        bucket_start: STOPWORD_BUCKET_START,
        data: STOPWORD_DATA,
        offsets_bin: STOPWORD_OFFSETS_BIN,
        count: STOPWORD_TERM_COUNT,
    }
}

/// Public read-only API for the bundled dictionaries. All operations are
/// `O(log N)` and never allocate.
pub struct Dict;

impl Dict {
    /// Total number of terms in the bundled main dictionary (0 if the
    /// `main-dict` feature is disabled).
    pub fn main_size() -> usize {
        MAIN_TERM_COUNT
    }

    /// Total number of terms in the bundled quantifier dictionary.
    pub fn quantifier_size() -> usize {
        QUANTIFIER_TERM_COUNT
    }

    /// Total number of terms in the bundled stopword dictionary.
    pub fn stopword_size() -> usize {
        STOPWORD_TERM_COUNT
    }

    /// The longest term character count across the bundled dictionaries.
    pub fn max_term_chars() -> usize {
        MAX_TERM_CHARS
    }

    /// Returns true if `word` is in the bundled main dictionary.
    pub fn contains_main(word: &str) -> bool {
        contains_in(&main_view(), word)
    }

    /// Returns true if `word` is in the bundled quantifier dictionary.
    pub fn contains_quantifier(word: &str) -> bool {
        contains_in(&quantifier_view(), word)
    }

    /// Returns true if `word` is in the bundled stopword dictionary.
    pub fn is_stopword(word: &str) -> bool {
        contains_in(&stopword_view(), word)
    }
}

fn contains_in(view: &DictView<'_>, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let first = word.chars().next().unwrap();
    let Some(bk) = view.bucket_of(first) else {
        return false;
    };
    let lo = view.bucket_start[bk] as u32;
    let hi = view.bucket_start[bk + 1] as u32;
    let len = (hi - lo) as usize;
    let pp = partition_point_in_bucket(view, lo, len, word.as_bytes());
    pp < len && view.term_bytes(lo + pp as u32) == word.as_bytes()
}

/// Iterate every term in `view` that starts at byte offset 0 in `text` (i.e.
/// every dict term that is a prefix of `text`). The callback is invoked
/// with the matching term as a `&'a [u8]`.
pub(crate) fn for_each_prefix_in<'a, F: FnMut(&'a [u8])>(
    view: &DictView<'a>,
    text: &'a [u8],
    f: F,
) {
    view.find_prefixes(text, f);
}

/// Membership test against an arbitrary [`DictView`].
pub(crate) fn view_contains(view: &DictView<'_>, word: &str) -> bool {
    contains_in(view, word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_dict_loaded() {
        if cfg!(feature = "main-dict") {
            assert!(Dict::main_size() > 100_000);
        }
    }

    #[test]
    fn stopword_loaded() {
        assert!(Dict::stopword_size() > 0);
    }

    #[test]
    fn contains_known_word() {
        if cfg!(feature = "main-dict") {
            assert!(Dict::contains_main("中国"));
            assert!(Dict::contains_main("人民"));
        }
    }
}
