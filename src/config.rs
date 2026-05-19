//! Configuration for [`crate::IkTokenizer`].

/// Tokenization mode. Matches the two modes exposed by the Java IK plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IkMode {
    /// `ik_smart` — pick the best (single) segmentation path via the
    /// arbitrator. Few, long, non-overlapping tokens — good for queries.
    Smart,
    /// `ik_max_word` — emit every dict hit. Many, possibly overlapping
    /// tokens — good for indexing because it maximizes recall.
    MaxWord,
}

impl Default for IkMode {
    fn default() -> Self {
        IkMode::MaxWord
    }
}

/// Runtime configuration. Construct via [`IkConfig::default`] or builder
/// methods.
#[derive(Debug, Clone)]
pub struct IkConfig {
    /// Tokenization mode.
    pub mode: IkMode,
    /// Lowercase ASCII letters when emitting tokens (also drives the
    /// `regularize` step on the input).
    pub lowercase: bool,
    /// When `true`, tokens that exactly match an entry in the bundled
    /// stopword dictionary (or a user-added stopword) are dropped.
    pub use_stopwords: bool,
    /// When `true` AND `mode == MaxWord`, additionally emit:
    ///   - every Chinese-number + quantifier fusion `Smart` would create
    ///     (e.g. `三个`, `五公斤`), as `CnQuan` tokens, *alongside* the
    ///     constituent `CnNum` + `Count` lexemes; and
    ///   - a single-char `CnChar` at every Chinese position (even when it
    ///     is already covered by a longer word).
    ///
    /// This is what guarantees the `Smart ⊆ MaxWord` invariant — every
    /// token a `Smart` query produces is also in a `MaxWord` index. Has no
    /// effect when `mode == Smart`.
    ///
    /// Cost: roughly 1.5–2× more emitted tokens on heavy-CJK text. Disable
    /// if you don't need the invariant and want a smaller index.
    pub ensure_smart_subset: bool,
}

impl Default for IkConfig {
    fn default() -> Self {
        Self {
            mode: IkMode::MaxWord,
            lowercase: true,
            use_stopwords: false,
            ensure_smart_subset: true,
        }
    }
}

impl IkConfig {
    /// Indexing-friendly preset: `ik_max_word`, lowercase, stopwords off,
    /// `ensure_smart_subset` ON (so a `Smart` query is guaranteed to find
    /// every token).
    pub fn for_indexing() -> Self {
        Self {
            mode: IkMode::MaxWord,
            lowercase: true,
            use_stopwords: false,
            ensure_smart_subset: true,
        }
    }

    /// Search-friendly preset: `ik_smart`, lowercase, stopwords on.
    pub fn for_searching() -> Self {
        Self {
            mode: IkMode::Smart,
            lowercase: true,
            use_stopwords: true,
            ensure_smart_subset: true,
        }
    }

    pub fn mode(mut self, mode: IkMode) -> Self {
        self.mode = mode;
        self
    }

    pub fn lowercase(mut self, lc: bool) -> Self {
        self.lowercase = lc;
        self
    }

    pub fn use_stopwords(mut self, sw: bool) -> Self {
        self.use_stopwords = sw;
        self
    }

    /// Opt out of the `Smart ⊆ MaxWord` indexing-time guarantee. Saves
    /// 1.5–2× emitted tokens on heavy-CJK text at the cost of the
    /// subset property.
    pub fn ensure_smart_subset(mut self, on: bool) -> Self {
        self.ensure_smart_subset = on;
        self
    }
}
