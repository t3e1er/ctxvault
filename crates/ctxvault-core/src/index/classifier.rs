//! Deterministic corpus modality classifier.
//!
//! Classifies files into documentation notes, polyglot code, rich documents
//! (.html, .pdf, .docx), or ignored files using a 3-tier hierarchy:
//! 1. Explicit directory patterns (`doc_patterns`, `code_patterns`).
//! 2. Corpus role/type (`CodeRepo`, `DocVault`, `Mixed`).
//! 3. Content-based text-to-tag heuristics for dual-nature formats (e.g. HTML).

use std::path::Path;

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use ctxvault_common::config::{CorpusConfig, CorpusType};
use ctxvault_common::types::FileFormat;

use crate::parser::code::{detect_language, is_code_file, SupportedLanguage};

/// Categorization of a file discovered in a corpus root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileClassification {
    /// Native Markdown documentation note (.md).
    MarkdownDoc,
    /// Polyglot source code file.
    Code(SupportedLanguage),
    /// Rich document file that will produce a Derived Text Projection.
    Document(FileFormat),
    /// Ignored file (binary asset, test fixture, or unsupported format).
    Ignored,
}

impl FileClassification {
    /// Returns true if this file is indexable (doc, code, or rich document).
    pub fn is_indexable(&self) -> bool {
        !matches!(self, Self::Ignored)
    }

    /// Returns the corresponding `FileFormat`.
    pub fn file_format(&self) -> FileFormat {
        match self {
            Self::MarkdownDoc | Self::Code(_) => FileFormat::Source,
            Self::Document(fmt) => *fmt,
            Self::Ignored => FileFormat::Source,
        }
    }

    /// Returns whether this file is treated as documentation for retrieval modality.
    pub fn is_documentation(&self) -> bool {
        matches!(self, Self::MarkdownDoc | Self::Document(_))
    }
}

/// A compiled classifier for determining file modalities.
#[derive(Clone, Debug)]
pub struct FileClassifier {
    corpus_type: CorpusType,
    doc_matcher: Option<Gitignore>,
    code_matcher: Option<Gitignore>,
}

impl FileClassifier {
    /// Build a new classifier from the corpus configuration.
    pub fn new(root: &Path, config: &CorpusConfig) -> Self {
        let doc_matcher = if !config.doc_patterns.is_empty() {
            let mut builder = GitignoreBuilder::new(root);
            for p in &config.doc_patterns {
                let _ = builder.add_line(None, p);
            }
            builder.build().ok()
        } else {
            None
        };

        let code_matcher = if !config.code_patterns.is_empty() {
            let mut builder = GitignoreBuilder::new(root);
            for p in &config.code_patterns {
                let _ = builder.add_line(None, p);
            }
            builder.build().ok()
        } else {
            None
        };

        Self { corpus_type: config.corpus_type, doc_matcher, code_matcher }
    }

    /// Classify a file by path, optionally reading sample bytes for content heuristic.
    pub fn classify(&self, path: &Path, sample_bytes: Option<&[u8]>) -> FileClassification {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();

        // 1. Native Markdown notes are always documentation.
        if ext == "md" || ext == "markdown" {
            return FileClassification::MarkdownDoc;
        }

        let is_html = ext == "html" || ext == "htm";
        let is_pdf = ext == "pdf";
        let is_docx = ext == "docx";

        // 2. Non-rich code files
        if !is_html && !is_pdf && !is_docx {
            if is_code_file(path) {
                if let Some(lang) = detect_language(path) {
                    return FileClassification::Code(lang);
                }
            }
            return FileClassification::Ignored;
        }

        // 3. Explicit pattern matching (doc_patterns take priority over code_patterns)
        if let Some(ref matcher) = self.doc_matcher {
            if matcher.matched_path_or_any_parents(path, false).is_ignore() {
                if is_html {
                    return FileClassification::Document(FileFormat::HtmlDoc);
                } else if is_pdf {
                    return FileClassification::Document(FileFormat::Pdf);
                } else if is_docx {
                    return FileClassification::Document(FileFormat::Docx);
                }
            }
        }

        if let Some(ref matcher) = self.code_matcher {
            if matcher.matched_path_or_any_parents(path, false).is_ignore() {
                if is_html {
                    return FileClassification::Code(SupportedLanguage::Html);
                } else {
                    return FileClassification::Ignored;
                }
            }
        }

        // 4. Default corpus type routing
        match self.corpus_type {
            CorpusType::DocVault => {
                if is_html {
                    FileClassification::Document(FileFormat::HtmlDoc)
                } else if is_pdf {
                    FileClassification::Document(FileFormat::Pdf)
                } else if is_docx {
                    FileClassification::Document(FileFormat::Docx)
                } else {
                    FileClassification::Ignored
                }
            }
            CorpusType::CodeRepo => {
                if is_html {
                    FileClassification::Code(SupportedLanguage::Html)
                } else {
                    FileClassification::Ignored
                }
            }
            CorpusType::Mixed => {
                if is_pdf {
                    FileClassification::Document(FileFormat::Pdf)
                } else if is_docx {
                    FileClassification::Document(FileFormat::Docx)
                } else if is_html {
                    if let Some(bytes) = sample_bytes {
                        if is_html_documentation_article(bytes) {
                            FileClassification::Document(FileFormat::HtmlDoc)
                        } else {
                            FileClassification::Code(SupportedLanguage::Html)
                        }
                    } else {
                        // Without bytes, assume HTML in mixed repo is documentation if in docs-like path
                        let path_str = path.to_string_lossy().to_ascii_lowercase();
                        if path_str.contains("doc")
                            || path_str.contains("wiki")
                            || path_str.contains("spec")
                        {
                            FileClassification::Document(FileFormat::HtmlDoc)
                        } else {
                            FileClassification::Code(SupportedLanguage::Html)
                        }
                    }
                } else {
                    FileClassification::Ignored
                }
            }
        }
    }
}

/// Inspect up to 4 KB of HTML to heuristically determine if it is an article/documentation.
pub fn is_html_documentation_article(bytes: &[u8]) -> bool {
    let sample_len = bytes.len().min(4096);
    let sample = String::from_utf8_lossy(&bytes[..sample_len]).to_ascii_lowercase();

    // Check code/UI indicators
    if sample.contains("<template")
        || sample.contains("<router-outlet")
        || sample.contains("ng-")
        || sample.contains("v-if")
        || sample.contains("v-for")
    {
        return false;
    }

    // Check documentation semantic elements
    if sample.contains("<article")
        || sample.contains("<main")
        || sample.contains("<section")
        || (sample.contains("<h1") && sample.contains("<p"))
    {
        return true;
    }

    // Calculate tag-to-text ratio
    let mut in_tag = false;
    let mut tag_chars = 0;
    let mut text_chars = 0;

    for ch in sample.chars() {
        match ch {
            '<' => {
                in_tag = true;
                tag_chars += 1;
            }
            '>' => {
                in_tag = false;
                tag_chars += 1;
            }
            _ if in_tag => {
                tag_chars += 1;
            }
            c if !c.is_whitespace() => {
                text_chars += 1;
            }
            _ => {}
        }
    }

    let total = tag_chars + text_chars;
    if total == 0 {
        return false;
    }

    let text_ratio = text_chars as f32 / total as f32;
    text_ratio > 0.40
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_classifier_code_repo_defaults() {
        let tmp = TempDir::new().unwrap();
        let config = CorpusConfig {
            name: "test".to_string(),
            path: tmp.path().to_string_lossy().to_string(),
            mode: Default::default(),
            index_mode: Default::default(),
            chunking: Default::default(),
            embedding: Default::default(),
            graph: Default::default(),
            templates_dir: None,
            exclude: Default::default(),
            corpus_type: CorpusType::CodeRepo,
            doc_patterns: vec!["docs/**".to_string()],
            code_patterns: vec!["src/**".to_string()],
        };

        let classifier = FileClassifier::new(tmp.path(), &config);

        // Markdown
        assert_eq!(
            classifier.classify(Path::new("README.md"), None),
            FileClassification::MarkdownDoc
        );

        // Rust code
        assert_eq!(
            classifier.classify(Path::new("src/main.rs"), None),
            FileClassification::Code(SupportedLanguage::Rust)
        );

        // HTML in src (matches code_patterns)
        assert_eq!(
            classifier.classify(Path::new("src/index.html"), None),
            FileClassification::Code(SupportedLanguage::Html)
        );

        // HTML in docs (matches doc_patterns)
        assert_eq!(
            classifier.classify(Path::new("docs/api.html"), None),
            FileClassification::Document(FileFormat::HtmlDoc)
        );

        // PDF in tests/fixtures (unmatched in CodeRepo -> Ignored)
        assert_eq!(
            classifier.classify(Path::new("tests/fixtures/sample.pdf"), None),
            FileClassification::Ignored
        );

        // PDF in docs (matches doc_patterns -> Document)
        assert_eq!(
            classifier.classify(Path::new("docs/manual.pdf"), None),
            FileClassification::Document(FileFormat::Pdf)
        );
    }

    #[test]
    fn test_classifier_doc_vault() {
        let tmp = TempDir::new().unwrap();
        let config = CorpusConfig {
            name: "test".to_string(),
            path: tmp.path().to_string_lossy().to_string(),
            mode: Default::default(),
            index_mode: Default::default(),
            chunking: Default::default(),
            embedding: Default::default(),
            graph: Default::default(),
            templates_dir: None,
            exclude: Default::default(),
            corpus_type: CorpusType::DocVault,
            doc_patterns: Vec::new(),
            code_patterns: Vec::new(),
        };

        let classifier = FileClassifier::new(tmp.path(), &config);
        assert_eq!(
            classifier.classify(Path::new("specs/architecture.docx"), None),
            FileClassification::Document(FileFormat::Docx)
        );
        assert_eq!(
            classifier.classify(Path::new("papers/attention.pdf"), None),
            FileClassification::Document(FileFormat::Pdf)
        );
        assert_eq!(
            classifier.classify(Path::new("site/page.html"), None),
            FileClassification::Document(FileFormat::HtmlDoc)
        );
    }

    #[test]
    fn test_html_content_heuristic() {
        let doc_html = b"<!DOCTYPE html><html><body><article><h1>Architecture Overview</h1><p>This is a comprehensive document describing the system architecture and design principles in great detail.</p></article></body></html>";
        assert!(is_html_documentation_article(doc_html));

        let ui_html = b"<div class=\"container\"><template v-if=\"active\"><button @click=\"submit\">Submit</button></template></div>";
        assert!(!is_html_documentation_article(ui_html));
    }
}
