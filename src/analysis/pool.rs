use std::collections::HashMap;
use std::collections::hash_map::Entry;

use anyhow::Result;
use tree_sitter::Parser;

use super::languages::Language;

/// Pool of tree-sitter Parser instances cached by language.
/// Avoids recreating and reconfiguring Parser on every file scan.
/// Parser internally reuses allocations, so caching is worthwhile.
pub struct ParserPool {
    parsers: HashMap<Language, Parser>,
}

impl ParserPool {
    pub fn new() -> Self {
        Self {
            parsers: HashMap::new(),
        }
    }

    /// Get (or create and cache) a configured parser for the given language.
    pub fn acquire(&mut self, lang: Language) -> Result<&mut Parser> {
        if let Entry::Vacant(e) = self.parsers.entry(lang) {
            let mut parser = Parser::new();
            let ts_lang = super::extractors::ts_language(lang);
            parser.set_language(&ts_lang)?;
            e.insert(parser);
        }
        Ok(self.parsers.get_mut(&lang).unwrap())
    }
}

impl Default for ParserPool {
    fn default() -> Self {
        Self::new()
    }
}
