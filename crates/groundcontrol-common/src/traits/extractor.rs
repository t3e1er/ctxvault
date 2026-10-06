//! Document and content extractor domain capability contract for container formats (.docx, .pdf, .html).

use std::path::Path;

use crate::types::ExtractedDocument;
use crate::Result;

/// Domain capability contract for deterministic document and content extraction.
///
/// Implementations extract structured, normalized UTF-8 text and outbound links
/// from binary or container document formats (.docx, .pdf, .html) in 100% pure
/// Rust without external C runtimes.
pub trait DocumentExtractor: Send + Sync {
    /// Returns true if this extractor handles the given file format.
    fn can_extract(&self, path: &Path) -> bool;

    /// Extract structured text and links from raw file bytes.
    fn extract(&self, path: &Path, bytes: &[u8]) -> Result<ExtractedDocument>;
}

/// Canonical alias for [`DocumentExtractor`].
pub use DocumentExtractor as ContentExtractor;
