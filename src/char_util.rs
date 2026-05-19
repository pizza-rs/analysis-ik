//! Character classification & regularization, ported from
//! `org.wltea.analyzer.core.CharacterUtil` in the Java IK plugin.

/// Coarse character classification used by all the IK sub-segmenters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharKind {
    /// Whitespace / punctuation / unknown — segmenters reset on this.
    Useless,
    /// ASCII digit `'0'..='9'` (after full-width regularization).
    Arabic,
    /// ASCII letter `'a'..='z'` / `'A'..='Z'` (after full-width regularization).
    English,
    /// CJK unified ideograph or compatibility ideograph.
    Chinese,
    /// Hiragana, Katakana, Hangul, or other CJK forms.
    OtherCjk,
}

#[inline]
pub fn classify(c: char) -> CharKind {
    if c.is_ascii_digit() {
        CharKind::Arabic
    } else if c.is_ascii_alphabetic() {
        CharKind::English
    } else if is_cjk_ideograph(c) {
        CharKind::Chinese
    } else if is_other_cjk(c) {
        CharKind::OtherCjk
    } else {
        CharKind::Useless
    }
}

/// Fused regularize + classify with a fast path for the **CJK Unified
/// Ideographs** block (U+4E00..=U+9FFF). Real Chinese text is dominated by
/// codepoints in this range, all of which are invariant under `regularize`
/// (the full-width / ideographic-space / ASCII-case rewrites only fire
/// outside this block) and trivially classify as [`CharKind::Chinese`].
///
/// On the hot `CharStream::new` loop this collapses ~6 conditional range
/// checks into a single compare-and-branch for the common case, eliminating
/// the redundant work of calling `regularize` (3 checks) followed by
/// `classify` (4 checks) on every CJK character.
///
/// Returns `(regularized_char, kind)`. The caller can detect whether the
/// input was rewritten by comparing the returned char with the input — for
/// the CJK fast path this is trivially false and dead-code-eliminated.
#[inline]
pub fn regularize_and_classify(c: char, lowercase: bool) -> (char, CharKind) {
    let cp = c as u32;
    if cp >= 0x4E00 && cp <= 0x9FFF {
        // CJK Unified Ideographs: identity under regularize, always Chinese.
        return (c, CharKind::Chinese);
    }
    let r = regularize(c, lowercase);
    (r, classify(r))
}

#[inline]
fn is_cjk_ideograph(c: char) -> bool {
    let cp = c as u32;
    // CJK Unified Ideographs (U+4E00..=U+9FFF)
    // CJK Unified Ideographs Extension A (U+3400..=U+4DBF)
    // CJK Compatibility Ideographs (U+F900..=U+FAFF)
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
}

#[inline]
fn is_other_cjk(c: char) -> bool {
    let cp = c as u32;
    // Hangul syllables (U+AC00..=U+D7AF) + Hangul Jamo / Compatibility Jamo
    (0xAC00..=0xD7AF).contains(&cp)
        || (0x1100..=0x11FF).contains(&cp)
        || (0x3130..=0x318F).contains(&cp)
        // Hiragana / Katakana / Katakana Phonetic Ext
        || (0x3040..=0x309F).contains(&cp)
        || (0x30A0..=0x30FF).contains(&cp)
        || (0x31F0..=0x31FF).contains(&cp)
        // Halfwidth & Fullwidth Forms (without the ASCII range we already handle)
        || (0xFF00..=0xFFEF).contains(&cp)
}

/// Java IK's `regularize`: maps full-width ASCII to half-width and
/// (optionally) uppercase ASCII to lowercase. The input is one code point.
#[inline]
pub fn regularize(input: char, lowercase: bool) -> char {
    let cp = input as u32;
    if cp == 0x3000 {
        // Ideographic space → ASCII space.
        return ' ';
    }
    if (0xFF01..0xFF5F).contains(&cp) {
        // Full-width ASCII (! .. ~) → half-width.
        let mapped = (cp - 0xFEE0) as u32;
        if let Some(c) = char::from_u32(mapped) {
            return if lowercase && c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                c
            };
        }
    }
    if lowercase && input.is_ascii_uppercase() {
        return input.to_ascii_lowercase();
    }
    input
}

/// Quick test for IK's Chinese numeral characters used by the quantifier
/// segmenter. Matches `org.wltea.analyzer.core.CN_QuantifierSegmenter.Chn_Num`.
pub fn is_chinese_number(c: char) -> bool {
    matches!(
        c,
        '一' | '二'
            | '两'
            | '三'
            | '四'
            | '五'
            | '六'
            | '七'
            | '八'
            | '九'
            | '十'
            | '零'
            | '壹'
            | '贰'
            | '叁'
            | '肆'
            | '伍'
            | '陆'
            | '柒'
            | '捌'
            | '玖'
            | '拾'
            | '百'
            | '千'
            | '万'
            | '亿'
            | '佰'
            | '仟'
            | '萬'
            | '億'
            | '兆'
            | '卅'
            | '廿'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_works() {
        assert_eq!(classify('a'), CharKind::English);
        assert_eq!(classify('Z'), CharKind::English);
        assert_eq!(classify('5'), CharKind::Arabic);
        assert_eq!(classify('中'), CharKind::Chinese);
        assert_eq!(classify('あ'), CharKind::OtherCjk);
        assert_eq!(classify(','), CharKind::Useless);
        assert_eq!(classify(' '), CharKind::Useless);
    }

    #[test]
    fn regularize_full_width() {
        assert_eq!(regularize('Ａ', true), 'a');
        assert_eq!(regularize('Ｚ', false), 'Z');
        assert_eq!(regularize('５', true), '5');
        assert_eq!(regularize('　', true), ' ');
        assert_eq!(regularize('A', true), 'a');
        assert_eq!(regularize('A', false), 'A');
    }
}
