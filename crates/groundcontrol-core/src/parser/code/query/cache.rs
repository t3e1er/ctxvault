//! Thread-safe OnceLock compilation cache for Tree-sitter query packs.

use std::sync::OnceLock;
use tree_sitter::Query;

use super::LanguageQuery;
use crate::parser::code::languages::SupportedLanguage;

const LANG_COUNT: usize = 64;
static QUERIES: [OnceLock<Option<LanguageQuery>>; LANG_COUNT] =
    [const { OnceLock::new() }; LANG_COUNT];

fn compile_query(lang: SupportedLanguage, source: &str) -> Option<LanguageQuery> {
    let ts_lang = lang.tree_sitter_language();
    match Query::new(&ts_lang, source) {
        Ok(q) => Some(LanguageQuery::new(q)),
        Err(err) => {
            tracing::error!(
                "Failed to compile tree-sitter query pack for {}: {:?}",
                lang.name(),
                err
            );
            None
        }
    }
}

/// Retrieve the pre-compiled LanguageQuery for a given language, if available.
pub fn get_language_query(lang: SupportedLanguage) -> Option<&'static LanguageQuery> {
    let idx = lang as usize;
    if idx < LANG_COUNT {
        QUERIES[idx]
            .get_or_init(|| {
                let src = lang.query_source();
                compile_query(lang, &src)
            })
            .as_ref()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_languages_compile_queries() {
        use crate::parser::code::languages::SupportedLanguage;

        for lang in [
            SupportedLanguage::Xml,
            SupportedLanguage::Vb6,
            SupportedLanguage::PlSql,
            SupportedLanguage::Cobol,
        ] {
            assert!(get_language_query(lang).is_some(), "Query pack should compile for {:?}", lang);
        }
    }
}
