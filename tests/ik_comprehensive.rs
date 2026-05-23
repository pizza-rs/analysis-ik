//! Comprehensive tests for pizza-analysis-ik (IK Chinese analyzer).

use pizza_analysis_ik::{IkConfig, IkMode, IkTokenizer, Rules};
use pizza_engine::analysis::{AnalysisFactory, Token, Tokenizer};

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn terms(tokens: &[Token]) -> Vec<String> {
    tokens.iter().map(|t| t.term.to_string()).collect()
}

// ═══════════════════════════════════════════════════════════════════════════════
// IkConfig
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn config_default() {
    let cfg = IkConfig::default();
    assert_eq!(cfg.mode, IkMode::MaxWord);
    assert!(cfg.lowercase);
    assert!(!cfg.use_stopwords);
}

#[test]
fn config_for_indexing() {
    let cfg = IkConfig::for_indexing();
    assert_eq!(cfg.mode, IkMode::MaxWord);
    assert!(cfg.ensure_smart_subset);
}

#[test]
fn config_for_searching() {
    let cfg = IkConfig::for_searching();
    assert_eq!(cfg.mode, IkMode::Smart);
    assert!(cfg.use_stopwords);
}

#[test]
fn config_builder_chain() {
    let cfg = IkConfig::default()
        .mode(IkMode::Smart)
        .lowercase(false)
        .use_stopwords(true)
        .ensure_smart_subset(false);
    assert_eq!(cfg.mode, IkMode::Smart);
    assert!(!cfg.lowercase);
    assert!(cfg.use_stopwords);
    assert!(!cfg.ensure_smart_subset);
}

// ═══════════════════════════════════════════════════════════════════════════════
// IkTokenizer — construction
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tokenizer_default() {
    let _t = IkTokenizer::default();
}

#[test]
fn tokenizer_with_defaults() {
    let _t = IkTokenizer::with_defaults();
}

#[test]
fn tokenizer_smart_mode() {
    let _t = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart));
}

#[test]
fn tokenizer_max_word_mode() {
    let _t = IkTokenizer::new(IkConfig::default().mode(IkMode::MaxWord));
}

#[test]
fn tokenizer_clone() {
    let t1 = IkTokenizer::with_defaults();
    let _t2 = t1.clone();
}

#[test]
fn tokenizer_debug() {
    let t = IkTokenizer::with_defaults();
    let dbg = format!("{:?}", t);
    assert!(dbg.contains("IkTokenizer"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// IkTokenizer — Smart mode
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn smart_basic_segmentation() {
    let t = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart));
    let tokens = t.tokenize("中华人民共和国");
    let ts = terms(&tokens);
    assert!(!ts.is_empty());
    // Smart mode produces non-overlapping tokens — verify all terms are substrings
    for s in &ts {
        assert!(
            "中华人民共和国".contains(s.as_str()),
            "unexpected token: '{}'",
            s
        );
    }
}

#[test]
fn smart_sentence() {
    let t = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart));
    let tokens = t.tokenize("我是中国人");
    let ts = terms(&tokens);
    assert!(!ts.is_empty());
}

#[test]
fn smart_mixed_chinese_ascii() {
    let t = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart));
    let tokens = t.tokenize("我喜欢Java编程");
    let ts = terms(&tokens);
    assert!(ts.iter().any(|s| s.to_lowercase() == "java"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// IkTokenizer — MaxWord mode
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn max_word_produces_more_tokens() {
    let smart = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart));
    let max_word = IkTokenizer::new(IkConfig::default().mode(IkMode::MaxWord));

    let text = "中华人民共和国";
    let smart_tokens = smart.tokenize(text);
    let mw_tokens = max_word.tokenize(text);

    // MaxWord should produce at least as many tokens as Smart
    assert!(mw_tokens.len() >= smart_tokens.len());
}

#[test]
fn max_word_overlapping_tokens() {
    let t = IkTokenizer::new(IkConfig::default().mode(IkMode::MaxWord));
    let tokens = t.tokenize("中华人民共和国");
    let ts = terms(&tokens);
    // MaxWord should produce sub-words like "中华", "人民", "共和国" etc.
    assert!(ts.len() > 1);
}

// ═══════════════════════════════════════════════════════════════════════════════
// IkTokenizer — edge cases
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tokenize_empty_string() {
    let t = IkTokenizer::with_defaults();
    let tokens = t.tokenize("");
    assert!(tokens.is_empty());
}

#[test]
fn tokenize_single_chinese_char() {
    let t = IkTokenizer::with_defaults();
    let tokens = t.tokenize("我");
    assert!(!tokens.is_empty());
    assert_eq!(tokens[0].term.as_ref(), "我");
}

#[test]
fn tokenize_pure_ascii() {
    let t = IkTokenizer::with_defaults();
    let tokens = t.tokenize("hello world");
    assert!(!tokens.is_empty());
}

#[test]
fn tokenize_pure_digits() {
    let t = IkTokenizer::with_defaults();
    let tokens = t.tokenize("12345");
    assert!(!tokens.is_empty());
}

#[test]
fn tokenize_chinese_numbers_with_quantifier() {
    let t = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart));
    let tokens = t.tokenize("三个苹果");
    assert!(!tokens.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════════
// IkTokenizer — offsets
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn offsets_valid() {
    let t = IkTokenizer::with_defaults();
    let text = "中华人民共和国成立于1949年";
    let tokens = t.tokenize(text);
    for tok in &tokens {
        assert!(tok.start_offset <= tok.end_offset);
        assert!((tok.end_offset as usize) <= text.len());
    }
}

#[test]
fn positions_valid() {
    let t = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart));
    let tokens = t.tokenize("搜索引擎技术");
    // In Smart mode, positions should be sequential
    if tokens.len() > 1 {
        for window in tokens.windows(2) {
            assert!(window[1].position >= window[0].position);
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Rules — user dictionary extensions
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn rules_new() {
    let r = Rules::new();
    let _ = format!("{:?}", r);
}

#[test]
fn rules_add_word() {
    let mut r = Rules::new();
    r.add_word("披萨");
    // Should be recognized in subsequent tokenization
}

#[test]
fn rules_add_stopword() {
    let mut r = Rules::new();
    r.add_stopword("的");
}

#[test]
fn rules_chaining() {
    let mut r = Rules::new();
    r.add_word("披萨").add_word("搜索引擎").add_stopword("的");
}

#[test]
fn tokenizer_with_custom_word() {
    let mut rules = Rules::new();
    rules.add_word("披萨搜索");

    let t = IkTokenizer::with_defaults().with_rules(rules);
    let tokens = t.tokenize("披萨搜索引擎");
    let ts = terms(&tokens);
    assert!(ts.iter().any(|s| s == "披萨搜索"));
}

#[test]
fn tokenizer_with_stopwords() {
    let mut rules = Rules::new();
    rules.add_stopword("的");

    let t = IkTokenizer::new(IkConfig::default().use_stopwords(true)).with_rules(rules);
    let tokens = t.tokenize("美丽的花");
    let ts = terms(&tokens);
    assert!(!ts.iter().any(|s| s == "的"), "'的' should be filtered out");
}

#[test]
fn rules_remove_word() {
    let mut r = Rules::new();
    r.remove_word("中华人民共和国");
    // The removed word should no longer be matched as a single token
}

// ═══════════════════════════════════════════════════════════════════════════════
// TokenRange API
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tokenize_ranges_basic() {
    let t = IkTokenizer::with_defaults();
    let text = "中华人民共和国";
    let ranges = t.tokenize_ranges(text);
    assert!(!ranges.is_empty());
    for r in &ranges {
        assert!(r.start_offset <= r.end_offset);
        assert!((r.end_offset as usize) <= text.len());
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Registration
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn register_all_does_not_panic() {
    let mut factory = AnalysisFactory::new();
    pizza_analysis_ik::register_all(&mut factory);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Smart ⊆ MaxWord invariant
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn smart_subset_of_max_word() {
    let smart = IkTokenizer::new(IkConfig::default().mode(IkMode::Smart).ensure_smart_subset(true));
    let mw = IkTokenizer::new(IkConfig::default().mode(IkMode::MaxWord).ensure_smart_subset(true));

    let text = "中国人民银行成立";
    let smart_terms: std::collections::HashSet<String> = terms(&smart.tokenize(text)).into_iter().collect();
    let mw_terms: std::collections::HashSet<String> = terms(&mw.tokenize(text)).into_iter().collect();

    for st in &smart_terms {
        assert!(
            mw_terms.contains(st),
            "Smart token '{}' not found in MaxWord output",
            st
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Unicode handling
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn tokenize_fullwidth_chars() {
    let t = IkTokenizer::with_defaults();
    let _tokens = t.tokenize("Ｈｅｌｌｏ");
}

#[test]
fn tokenize_emoji_mixed() {
    let t = IkTokenizer::with_defaults();
    let _tokens = t.tokenize("你好😊世界🌍");
}

#[test]
fn tokenize_mixed_scripts() {
    let t = IkTokenizer::with_defaults();
    let tokens = t.tokenize("Hello中国Мир");
    assert!(!tokens.is_empty());
}
