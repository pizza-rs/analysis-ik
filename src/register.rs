//! Register IK analysis components into [`AnalysisFactory`].

use alloc::boxed::Box;
use alloc::vec;

use pizza_engine::analysis::AnalysisFactory;
use pizza_engine::analysis::Analyzer;

use crate::IkConfig;
use crate::IkMode;
use crate::IkTokenizer;

/// Register IK tokenizers and analyzers.
pub fn register_all(factory: &mut AnalysisFactory) {
    // Tokenizers
    factory.register_tokenizer_with("ik_smart", || {
        Box::new(IkTokenizer::new(IkConfig::default().mode(IkMode::Smart)))
    });
    factory.register_tokenizer_with("ik_max_word", || {
        Box::new(IkTokenizer::new(IkConfig::default().mode(IkMode::MaxWord)))
    });

    // Analyzers (tokenizer-only, no extra filters)
    factory.register_analyzer_with("ik_smart", || {
        Analyzer::new(
            vec![],
            Box::new(IkTokenizer::new(IkConfig::default().mode(IkMode::Smart))),
            vec![],
        )
    });
    factory.register_analyzer_with("ik_max_word", || {
        Analyzer::new(
            vec![],
            Box::new(IkTokenizer::new(IkConfig::default().mode(IkMode::MaxWord))),
            vec![],
        )
    });
}
