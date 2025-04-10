// src/extractors/mod.rs
pub mod section;

// Re-export key extraction types for convenience
#[allow(unused_imports)]
pub use section::{
    ExtractedSection,
    SectionExtractor,
    ExtractionStrategy,
    PatternExtractionStrategy,
    TocExtractionStrategy,
    FinancialStatementExtractionStrategy,
    PartIIExtractionStrategy,
};