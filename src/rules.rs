//! Runtime extension rules — the IK equivalent of `ext_dict` / `ext_stopwords`.
//!
//! `Rules` lets you add or remove words at runtime without rebuilding the
//! bundled compact dictionaries. The internal layout is a small sorted
//! `Vec<String>` per category; lookups are `O(log N)` and never allocate.

use alloc::collections::BTreeSet;

/// User-supplied dictionary overlay applied on top of the bundled IK dicts.
///
/// - `extra_words` extend the main dictionary (they participate in CJK
///   tokenization just like a bundled word would).
/// - `extra_stopwords` extend the stopword filter (only active when
///   [`crate::IkConfig::use_stopwords`] is `true`).
/// - `removed_main` / `removed_stopwords` veto entries from the bundled
///   sets (used at lookup-time).
#[derive(Debug, Default, Clone)]
pub struct Rules {
    extra_words: BTreeSet<String>,
    extra_stopwords: BTreeSet<String>,
    removed_main: BTreeSet<String>,
    removed_stopwords: BTreeSet<String>,
}

impl Rules {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a new word so it gets recognized as a main-dict term.
    pub fn add_word(&mut self, word: impl Into<String>) -> &mut Self {
        let w = word.into();
        if !w.is_empty() {
            self.extra_words.insert(w);
        }
        self
    }

    /// Add multiple words.
    pub fn add_words<I, S>(&mut self, words: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for w in words {
            self.add_word(w);
        }
        self
    }

    /// Add a new stopword. Only active when `IkConfig::use_stopwords = true`.
    pub fn add_stopword(&mut self, word: impl Into<String>) -> &mut Self {
        let w = word.into();
        if !w.is_empty() {
            self.extra_stopwords.insert(w);
        }
        self
    }

    /// Block a bundled main-dict word from being matched.
    pub fn remove_word(&mut self, word: impl Into<String>) -> &mut Self {
        let w = word.into();
        if !w.is_empty() {
            self.removed_main.insert(w);
        }
        self
    }

    /// Block a bundled stopword from being filtered.
    pub fn remove_stopword(&mut self, word: impl Into<String>) -> &mut Self {
        let w = word.into();
        if !w.is_empty() {
            self.removed_stopwords.insert(w);
        }
        self
    }

    #[inline]
    pub(crate) fn is_extra_word(&self, w: &str) -> bool {
        self.extra_words.contains(w)
    }

    /// Fast-path predicate: true when no user-added extras exist. The
    /// MaxWord hot loop uses this to skip the per-position
    /// `for_each_extra_prefix` walk (and its per-call `String` allocation
    /// inside `range`) entirely.
    #[inline]
    pub(crate) fn has_extra_words(&self) -> bool {
        !self.extra_words.is_empty()
    }

    #[inline]
    pub(crate) fn is_removed_main(&self, w: &str) -> bool {
        self.removed_main.contains(w)
    }

    /// Fast-path predicate: true when there are bundled words the user has
    /// blocked. Lets the hot loop skip building the term slice for every
    /// trie hit when the user removed nothing (the overwhelmingly common
    /// case).
    #[inline]
    pub(crate) fn has_removed_main(&self) -> bool {
        !self.removed_main.is_empty()
    }

    #[inline]
    pub(crate) fn is_extra_stopword(&self, w: &str) -> bool {
        self.extra_stopwords.contains(w)
    }

    #[inline]
    pub(crate) fn is_removed_stopword(&self, w: &str) -> bool {
        self.removed_stopwords.contains(w)
    }

    /// Iterate every extra word that is a prefix of `text` (`text` already
    /// positioned at the candidate start). Used by the CJK segmenter to mix
    /// extras into its hit set.
    pub(crate) fn for_each_extra_prefix<'a, F: FnMut(&'a str)>(&'a self, text: &str, mut f: F) {
        // BTreeSet has range queries we can use to limit comparison.
        let first_char_end = match text.chars().next() {
            Some(c) => c.len_utf8(),
            None => return,
        };
        let lo = &text[..first_char_end];
        // Build upper bound by incrementing the last byte of the first char.
        let mut hi = lo.to_owned();
        if let Some(last) = hi.pop() {
            if let Some(next) = char::from_u32(last as u32 + 1) {
                hi.push(next);
            } else {
                hi.push(last);
                hi.push('\u{10FFFF}');
            }
        }
        for w in self.extra_words.range::<str, _>((
            std::ops::Bound::Included(lo),
            std::ops::Bound::Excluded(hi.as_str()),
        )) {
            if text.starts_with(w.as_str()) {
                f(w.as_str());
            }
        }
    }
}
