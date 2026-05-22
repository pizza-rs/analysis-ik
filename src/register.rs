//! Register IK analysis components into [`AnalysisFactory`].

use alloc::boxed::Box;
use alloc::vec;

use pizza_engine::analysis::AnalysisFactory;
use pizza_engine::analysis::Analyzer;

use crate::{IkConfig, IkMode, IkTokenizer};

/// Register IK tokenizers and analyzers.
pub fn register_all(factory: &mut AnalysisFactory) {
    // Tokenizers
    factory.register_tokenizer("ik_smart", Box::new(IkTokenizer::new(IkConfig::default().mode(IkMode::Smart))));
    factory.register_tokenizer("ik_max_word", Box::new(IkTokenizer::new(IkConfig::default().mode(IkMode::MaxWord))));

    // Analyzers (tokenizer-only, no extra filters)
    factory.register_analyzer(
        "ik_smart",
        Analyzer::new(vec![], Box::new(IkTokenizer::new(IkConfig::default().mode(IkMode::Smart))), vec![]),
    );
    factory.register_analyzer(
        "ik_max_word",
        Analyzer::new(vec![], Box::new(IkTokenizer::new(IkConfig::default().mode(IkMode::MaxWord))), vec![]),
    );
}
