// src/extractors/section.rs
use crate::utils::error::ExtractError;
use once_cell::sync::Lazy;
use regex::Regex;

// --- Constants ---
const TOC_CHECK_RANGE: usize = 10000;
const START_VALIDATION_LOOKAHEAD: usize = 5000;
const END_SEARCH_BUFFER: usize = 100;
const FALLBACK_END_CHUNK_SIZE: usize = 350_000;
const TOC_POSITIONAL_CHECK_PERCENT: usize = 5; // Check if match is within first 5% of document

// --- Lazy-Initialized Regex Patterns ---

// ToC Detection Patterns
static TOC_INDICATORS_RE: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"(?i)<h[1-6][^>]*>\s*(?:table\s+of\s+contents|index|contents)\s*</h[1-6]>",
        r#"(?i)<div[^>]*class=['"]?(?:toc|tableofcontents|index)['"]?[^>]*>"#, // Common class names
        r#"(?i)<nav[^>]*class=['"]?(?:toc|tableofcontents)['"]?[^>]*>"#, // Common nav classes
        r"(?i)\btable\s+of\s+contents\b", // Plain text
        r"(?i)item\s+\d+\.?\s*[\s\.]{3,}\s*(?:page|pg\.?)\s*\d+", // Item...Page pattern with dot leaders
    ]
    .iter()
    .filter_map(|pat| Regex::new(pat).ok())
    .collect()
});

static END_TOC_MARKERS_RE: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"(?i)</div>\s*<(?:p|h[1-6]|table|ul|ol)", // End of a div followed by common block elements
        r"(?i)</nav>",
        r"(?i)<hr", // Includes page breaks often done with <hr>
        r"(?i)<h[1-6][^>]*>\s*PART\s+I\b",
        r"(?i)<h[1-6][^>]*>\s*Item\s+1\b",
        r###"(?i)<hr[^>]*class=['"'][^'"]*page-break[^'"]*['"']"###, // Explicit page break class
        r###"(?i)style=['"'][^'"]*page-break-before[^'"]*['"']"###, // Explicit page break style
        // Added: Look for start of main content sections
        r"(?i)<h[1-6][^>]*>\s*(?:Item\s+1\b|Business\b|Risk\s*Factors\b)",
    ]
    .iter()
    .filter_map(|pat| Regex::new(pat).ok())
    .collect()
});

// Item 8 Start Patterns - More specific first
static ITEM_8_START_RE: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        // Full title in specific tags
        r#"(?is)<h[1-6][^>]*>\s*Item\s*8\.?\s*Financial\s*Statements\s*(?:and\s*Supplementary\s*Data)?\s*</h[1-6]>"#,
        r#"(?is)<p[^>]*>\s*<b>\s*Item\s*8\.?\s*Financial\s*Statements\s*(?:and\s*Supplementary\s*Data)?\s*</b>\s*</p>"#, // Bold paragraph common pattern
        r#"(?is)(?:<p[^>]*>|<div[^>]*>)\s*(?:<b>|<strong>)\s*Item\s*8\.?\s*Financial\s*Statements\s*(?:and\s*Supplementary\s*Data)?\s*(?:</b>|</strong>)\s*(?:</p>|</div>)"#, // Other bold variations

        // "Item 8." inside specific tags
        r#"(?is)<h[1-6][^>]*>.*?\bItem\s*8\..*?</h[1-6]>"#,
        r#"(?is)(?:<p[^>]*>|<div[^>]*>|<span>|<font[^>]*>|<b>|<strong>)\s*\bItem\s*8\.\s*(?:</p>|</div>|</span>|</font>|</b>|</strong>|<)"#,

        // Text patterns (lower priority)
        r"(?i)\bItem\s*8[\.\s\-–—:]+Financial\s*Statements\s*(?:and\s*Supplementary\s*Data)?\b",
        r"(?i)\bItem\s*8\.", // Simplest text match
    ]
    .iter()
    .filter_map(|pat| Regex::new(pat).ok())
    .collect()
});

// Item 8 End Patterns - Keep as is (seemed okay)
static ITEM_8_END_RE: Lazy<Vec<Regex>> = Lazy::new(|| {
     [
        r#"(?is)<h[1-6][^>]*>\s*Item\s*9[ABC]?\.?\s*[Cc]hanges\b"#, // Item 9, 9A, 9B etc.
        r#"(?is)(?:<p[^>]*>|<div[^>]*>|<strong>)\s*Item\s*9[ABC]?\.?\s*[Cc]hanges\b"#,
        r"(?i)\bItem\s*9[ABC]?\.?\s*[–\-—\s:]*\s*[Cc]hanges\s*in\s*and\s*[Dd]isagreements",
        r"(?i)\bItem\s*9\.", // Simple Item 9. marker
        r#"(?is)<h[1-6][^>]*>\s*PART\s*III\b"#, // Start of Part III
        r#"(?is)(?:<p[^>]*>|<div[^>]*>|<strong>)\s*PART\s*III\b"#,
        r"(?i)\bPART\s+III\b", // Plain text PART III
        r#"(?is)<h[1-6][^>]*>\s*Item\s*10\.?\s*[Dd]irectors\b"#, // Start of Item 10
        r#"(?is)(?:<p[^>]*>|<div[^>]*>|<strong>)\s*Item\s*10\.?\s*[Dd]irectors\b"#,
        r"(?i)\bSIGNATURES\b", // Signatures section
        r"(?i)\bEXHIBIT\s+INDEX\b", // Exhibit Index section
        r"(?i)<h[1-6][^>]*>\s*EXHIBITS?\b", // Exhibits header
    ]
    .iter()
    .filter_map(|pat| Regex::new(pat).ok())
    .collect()
});

// *** NEW REGEXES FOR IMPROVED TOC CHECK ***
static TOC_CONTAINER_START_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?i)<(?:div|nav|ul|ol)[^>]*?(?:id|class)\s*=\s*['"]?(?:toc|tableofcontents|table-of-contents|index)['"]?"#)
        .expect("Failed to compile TOC_CONTAINER_START_RE")
});
static TOC_QUICK_END_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)</(?:div|nav|ul|ol)>|<hr\b|<h[1-6][^>]*>\s*PART\s+I\b|<h[1-6][^>]*>\s*Item\s+1\b")
        .expect("Failed to compile TOC_QUICK_END_RE")
});
// *** END NEW REGEXES ***


// --- Helper Functions ---

/// Checks if a position is likely within a table of contents section.
fn is_in_table_of_contents(html_content: &str, position: usize) -> bool {
    // 1. Positional Check (Check if very early in the document)
    let positional_threshold = (html_content.len() * TOC_POSITIONAL_CHECK_PERCENT) / 100;
    let min_absolute_pos_check = 500; // Don't consider anything before this absolute position as non-ToC easily
    if position < positional_threshold || position < min_absolute_pos_check {
         tracing::trace!("Potential ToC match at pos {} (within first {}% or first {} bytes)", position, TOC_POSITIONAL_CHECK_PERCENT, min_absolute_pos_check);
         // Simple extra check: is it inside an obvious ToC container near the start?
         let nearby_start = position.saturating_sub(500);
         // Check content *around* the position for quick context
         let nearby_end = (position + 100).min(html_content.len());
         if nearby_start < nearby_end { // Check range validity
             let surrounding = &html_content[nearby_start..nearby_end];
             // Check against the specific container regex
             if TOC_CONTAINER_START_RE.is_match(surrounding) {
                 tracing::debug!("Skipping potential ToC match at pos {} (early & near specific toc container pattern)", position);
                 return true;
             }
             // Keep original simple class check as fallback
             if surrounding.contains(r#"class="toc"#) || surrounding.contains(r#"class="tableofcontents"#) {
                 tracing::debug!("Skipping potential ToC match at pos {} (early & near simple toc class)", position);
                 return true;
            }
         }
         // If it's very early but no container found nearby, it's likely still ToC or intro material.
         // Returning true here is safer to avoid false positives early on.
         tracing::debug!("Skipping potential ToC match at pos {} (very early position)", position);
         return true;
    }


    // 2. ADDED CHECK: Look for containing ToC element
    let container_search_start = position.saturating_sub(2000); // Look back a reasonable distance
    if container_search_start < position { // Ensure range is valid
        let context_before = &html_content[container_search_start..position];

        // Find the *last* ToC container start tag *before* the position
        if let Some(container_match) = TOC_CONTAINER_START_RE.find_iter(context_before).last() {
            let container_start_pos = container_search_start + container_match.start();
            tracing::trace!("Found potential ToC container start near {} before position {}", container_start_pos, position);

            // Now check if an end marker exists *between* the container start and the current position
            // Ensure slice is valid
            if container_start_pos < position {
                let content_inside = &html_content[container_start_pos..position];
                if TOC_QUICK_END_RE.find(content_inside).is_none() {
                    // No closing tag or major break found between container start and position. Likely still inside.
                    tracing::debug!("Skipping potential ToC match at pos {} (appears inside ToC container starting near {} with no intermediate end marker)", position, container_start_pos);
                    return true; // It's inside the container
                } else {
                    tracing::trace!("Found an end marker between container start {} and position {}", container_start_pos, position);
                    // Continue to the next check, as we might be outside the specific container
                }
            }
        }
    }
    // END ADDED CHECK


    // 3. Original Structural Check (As fallback/further validation)
    let search_start = position.saturating_sub(TOC_CHECK_RANGE);
    // Ensure slice end is strictly less than slice start
    if search_start >= position {
        tracing::warn!("Invalid range for ToC indicator search (start={}, end={})", search_start, position);
        return false; // Cannot search, assume not in ToC
    }
    let content_before = &html_content[search_start..position];


    let mut toc_indicator_found_pos: Option<usize> = None;
     // Find the start position of the *latest* indicator found *before* the current position
     for re in TOC_INDICATORS_RE.iter() {
         if let Some(toc_match) = re.find_iter(content_before).last() {
             // Use the *absolute* start position of the indicator match
             let current_indicator_start_abs = search_start + toc_match.start();
             if toc_indicator_found_pos.map_or(true, |latest| current_indicator_start_abs > latest) {
                 toc_indicator_found_pos = Some(current_indicator_start_abs);
             }
         }
     }


    if let Some(absolute_toc_indicator_start) = toc_indicator_found_pos {
        tracing::trace!("Latest ToC indicator relative to main pos {} starts around abs pos {}", position, absolute_toc_indicator_start);

        let search_for_end_marker_start = absolute_toc_indicator_start; // Search from the indicator start

        // Ensure slice is valid and search range is reasonable
        if search_for_end_marker_start < position {
            // Define the area to search for an end marker: from the indicator start up to slightly beyond the current position
             let search_limit_end = (position + END_SEARCH_BUFFER).min(html_content.len()); // Look slightly beyond position for end marker
            if search_for_end_marker_start < search_limit_end { // Final check for valid range
                let content_between_indicator_and_pos_area = &html_content[search_for_end_marker_start..search_limit_end];

                let mut end_marker_found_before_pos = false;
                for end_re in END_TOC_MARKERS_RE.iter() {
                    if let Some(end_match) = end_re.find(content_between_indicator_and_pos_area) {
                        let absolute_end_marker_pos = search_for_end_marker_start + end_match.start();
                        // Check if this end marker occurs *before* our target position
                        if absolute_end_marker_pos < position {
                             tracing::trace!("Found ToC end marker '{}' at {} between indicator {} and position {}", end_re.as_str(), absolute_end_marker_pos, absolute_toc_indicator_start, position);
                            end_marker_found_before_pos = true;
                            break; // Found an end marker before the position, definitely not in ToC anymore
                        }
                        // If end marker is found *at or after* the position, it doesn't help us exclude the current position yet.
                    }
                }

                if !end_marker_found_before_pos {
                    // Found ToC indicator, but no clear end marker after it *before* our position
                    tracing::debug!("Fallback ToC Check: Potential ToC match at pos {} (found ToC indicator near {}, no clear end marker found *before* pos)", position, absolute_toc_indicator_start);
                    return true; // Assume it's still in ToC
                } else {
                    // Found end marker before position -> Not in ToC anymore
                    tracing::trace!("Position {} is after an end marker found before it (indicator started at {})", position, absolute_toc_indicator_start);
                    return false;
                }

            } else {
                 tracing::warn!("Structural ToC Check: Invalid range for end marker search ({}-{})", search_for_end_marker_start, search_limit_end);
                 return false; // Cannot search effectively
            }
        } else {
             tracing::warn!("Structural ToC Check: Invalid range - end marker search start ({}) not before position ({})", search_for_end_marker_start, position);
             // This might happen if the indicator is *immediately* before the position. Treat as ToC.
             return true; // Safer to assume it's ToC if indicator is right before it.
        }
    } else {
        tracing::trace!("No relevant ToC indicator found before position {}", position);
    }

    false // Default: assume not in ToC if no checks returned true
}


/// Validates if the text slice looks like it contains financial data. (Keep as is)
fn contains_financial_content(text_slice: &str) -> bool {
     if text_slice.is_empty() { return false; }
    let lower_content = text_slice.to_lowercase();
    // Check for common financial statement titles
    let has_statement_titles = lower_content.contains("consolidated balance sheet")
        || lower_content.contains("consolidated statement of operations")
        || lower_content.contains("consolidated statement of income")
        || lower_content.contains("consolidated statement of cash flow")
        || lower_content.contains("consolidated statements of cash flows") // Plural variation
        || lower_content.contains("consolidated statement of stockholders' equity")
        || lower_content.contains("consolidated statement of shareholders' equity")
        || lower_content.contains("consolidated statements of comprehensive income")
        || lower_content.contains("consolidated statements of comprehensive loss");

    // Check for typical audit report phrasing
    let has_audit_report = lower_content.contains("report of independent registered public accounting firm")
        || (lower_content.contains("opinion") && lower_content.contains("audit") && lower_content.contains("financial statement"));

    // Check for notes section title
    let has_notes = lower_content.contains("notes to consolidated financial statements")
                 || lower_content.contains("notes to financial statements"); // Simpler variation

    // Check for presence of tables containing typical financial keywords (requires minimal length)
    let has_tables_with_keywords = if text_slice.contains("<table") {
        lower_content.contains("assets")
        || lower_content.contains("liabilities")
        || lower_content.contains("equity")
        || lower_content.contains("revenue")
        || lower_content.contains("expense")
        || lower_content.contains("net income")
        || lower_content.contains("net loss")
        || lower_content.contains("cash flow")
    } else { false };

    // Combine checks: Must have at least one strong indicator OR tables with keywords and sufficient length
    let is_valid = has_statement_titles || has_audit_report || has_notes || (has_tables_with_keywords && text_slice.len() > 2000);

     tracing::trace!(
         "Financial content validation: has_statements={}, has_audit={}, has_notes={}, has_tables_keywords={}, len={}, result={}",
         has_statement_titles, has_audit_report, has_notes, has_tables_with_keywords, text_slice.len(), is_valid
     );
     is_valid
}

// --- Extraction Logic ---
#[derive(Debug, Clone)]
pub struct ExtractedSection {
    pub section_name: String,   // e.g., "Item 8"
    pub section_title: String,  // e.g., "Financial Statements and Supplementary Data"
    pub content: String,        // The raw HTML content
    pub filing_year: u32,       // The year of the filing
    pub company_name: String,   // Company name
    pub ticker: String,         // Ticker symbol
}

/// Attempts to find the start and end byte positions of Item 8.
fn find_item_8_bounds(html_content: &str) -> Option<(usize, usize)> {
    let mut first_valid_start: Option<(usize, &str)> = None; // Store (position, pattern_str)

    // --- Find the best START position ---
    'outer_start: for start_re in ITEM_8_START_RE.iter() {
        for mat in start_re.find_iter(html_content) {
            let potential_start_pos = mat.start();
            let match_end_pos = mat.end(); // Where the start pattern match ends

            tracing::trace!("Considering potential start at {} (match ends {}) with pattern: '{}'", potential_start_pos, match_end_pos, start_re.as_str());

            // ** Check if likely in Table of Contents **
            if is_in_table_of_contents(html_content, potential_start_pos) {
                tracing::debug!("Skipping potential start at {} - Failed ToC check (pattern: '{}').", potential_start_pos, start_re.as_str());
                continue; // Skip this match, try next match or pattern
            }

            // ** Look ahead for financial content validation **
            // Ensure lookahead range is valid
            let lookahead_start = match_end_pos; // Start validation right after the matched pattern
            let lookahead_end = (lookahead_start + START_VALIDATION_LOOKAHEAD).min(html_content.len());

            if lookahead_start >= lookahead_end {
                tracing::trace!("Skipping potential Item 8 start at {} - lookahead range invalid (start={}, end={})", potential_start_pos, lookahead_start, lookahead_end);
                continue; // Skip, range is empty or invalid
            }
            let preview_content = &html_content[lookahead_start..lookahead_end];

            // Check if the preview content looks like actual financial data start
            if contains_financial_content(preview_content) {
                // ** Found a valid candidate **
                 tracing::info!(
                     "Found validated Item 8 start candidate at {} with pattern: '{}'",
                     potential_start_pos,
                     start_re.as_str()
                 );
                 // Store the *first* valid start found and break loops
                 first_valid_start = Some((potential_start_pos, start_re.as_str()));
                 break 'outer_start; // Found the first valid start, no need to check weaker patterns

            } else {
                tracing::debug!(
                    "Skipping potential Item 8 start at {} (pattern: '{}') - no financial content found in preview.",
                    potential_start_pos,
                    start_re.as_str()
                );
                 // Continue searching for other potential start matches with this pattern or next patterns
            }
        } // End loop through matches for one pattern
    } // End loop through start patterns

    // --- If no valid start was found, return None ---
    let (start_pos, start_pattern_str) = match first_valid_start {
        Some((pos, pattern)) => (pos, pattern),
        None => {
            tracing::warn!("No validated Item 8 start marker found after checking all patterns.");
            return None;
        }
    };
    tracing::info!("Selected Item 8 start position: {} (using pattern: '{}')", start_pos, start_pattern_str);


    // --- Find the END position ---
    // Search for the end marker starting slightly after the validated start position
    let search_for_end_from = (start_pos + END_SEARCH_BUFFER).min(html_content.len());
    let mut end_pos: Option<usize> = None;
    let mut end_marker_pattern_str: Option<&str> = None;

    // Search only in the relevant part of the document
    if search_for_end_from < html_content.len() {
        let search_area = &html_content[search_for_end_from..];
        for end_re in ITEM_8_END_RE.iter() {
            if let Some(end_mat) = end_re.find(search_area) {
                let potential_end_pos_relative = end_mat.start();
                let potential_end_pos_absolute = search_for_end_from + potential_end_pos_relative;

                tracing::trace!(
                    "Found potential end marker at abs {} (rel {}) with pattern: '{}'",
                    potential_end_pos_absolute, potential_end_pos_relative, end_re.as_str()
                );
                // We want the *earliest* end marker found after the start buffer
                if end_pos.map_or(true, |current| potential_end_pos_absolute < current) {
                    end_pos = Some(potential_end_pos_absolute);
                    end_marker_pattern_str = Some(end_re.as_str());
                    tracing::trace!("Updating earliest end marker to {} with pattern: '{}'", potential_end_pos_absolute, end_re.as_str());
                }
            }
        }
    }

    let final_end_pos = end_pos.unwrap_or_else(|| {
        // If no end marker found, use a fallback chunk size or end of document
        let fallback_end = (start_pos + FALLBACK_END_CHUNK_SIZE).min(html_content.len());
        tracing::warn!(
            "Item 8 end marker not found after pos {}. Using fallback end: {}",
            start_pos, fallback_end
        );
        fallback_end
    });

    if let Some(pattern_str) = end_marker_pattern_str {
         tracing::info!("Selected earliest end marker at pos {} using pattern: {}", final_end_pos, pattern_str);
    }

    // Final validation of bounds
    if final_end_pos <= start_pos {
         tracing::error!("Item 8 end position ({}) is not after start position ({}). Cannot extract.", final_end_pos, start_pos);
         return None; // Invalid bounds
    }

    tracing::info!("Returning final bounds: Start={}, End={}", start_pos, final_end_pos);
    Some((start_pos, final_end_pos))
}


/// Main extractor structure
pub struct SectionExtractor;

impl SectionExtractor {
    pub fn new() -> Self { Self {} }

    /// Extracts Item 8 content using the primary refined strategy.
    pub fn extract_item_8(
        &self,
        html_content: &str,
        filing_year: u32,
        company_name: &str,
        ticker: &str,
        min_section_size: usize,
    ) -> Result<ExtractedSection, ExtractError> {
        tracing::info!("Attempting to extract Item 8 for ticker {} ({}) with min size {}", ticker, filing_year, min_section_size);

        match find_item_8_bounds(html_content) {
            Some((start_pos, end_pos)) => {
                // Ensure end_pos doesn't exceed content length (should be handled by find_item_8_bounds, but double-check)
                 let final_end_pos = end_pos.min(html_content.len());

                if start_pos >= final_end_pos {
                     tracing::error!("Invalid bounds after adjustment: Start={}, End={}", start_pos, final_end_pos);
                     return Err(ExtractError::SectionNotFound("Invalid section bounds calculated".to_string()));
                 }

                let content_slice = &html_content[start_pos..final_end_pos];
                let section_size = final_end_pos - start_pos;

                // Check minimum size requirement
                if section_size < min_section_size {
                     tracing::error!("Extracted Item 8 section is too small ({} bytes, required {}) for ticker {} ({}). Start: {}, End: {}", section_size, min_section_size, ticker, filing_year, start_pos, final_end_pos);
                     return Err(ExtractError::SectionNotFound(format!("Item 8 found but size {} bytes is less than minimum {} bytes", section_size, min_section_size)));
                }

                // Final content validation on the *entire* extracted slice as a safety net
                // Although preview was checked, ensure the full slice still looks right.
                if !contains_financial_content(content_slice) {
                    tracing::warn!("Extracted Item 8 section ({} bytes) for ticker {} ({}) lacks strong financial content indicators after final bounding. Start: {}, End: {}. Discarding.", section_size, ticker, filing_year, start_pos, final_end_pos);
                     // Decide whether to error or return with warning. Error is safer.
                     return Err(ExtractError::SectionNotFound("Item 8 section found but failed final content validation".to_string()));
                }

                tracing::info!("Successfully extracted Item 8 for ticker {} ({}): {} bytes, Start: {}, End: {}", ticker, filing_year, section_size, start_pos, final_end_pos);
                Ok(ExtractedSection {
                    section_name: "Item 8".to_string(),
                    section_title: "Financial Statements and Supplementary Data".to_string(), // Could try to extract actual title later
                    content: content_slice.to_string(),
                    filing_year,
                    company_name: company_name.to_string(),
                    ticker: ticker.to_string(),
                })
            }
            None => {
                tracing::error!("Failed to find valid Item 8 boundaries for ticker {} ({})", ticker, filing_year);
                // Make error message slightly more informative
                Err(ExtractError::SectionNotFound(format!("No validated Item 8 start marker found for {}-{}", ticker, filing_year)))
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    // Use a smaller min size for tests unless specifically testing size limits
    const TEST_MIN_SIZE: usize = 50;

    // Helper to create basic HTML structure for testing boundary detection
     fn create_test_html(item8_header: &str, item8_body: &str, item9_header: &str) -> String {
         format!(r#"
         <!DOCTYPE html>
         <html>
         <head><title>Test 10-K Filing</title></head>
         <body>
             <h1>PART I</h1>
             <h2>Item 1. Business</h2>
             <p>Business description goes here...</p>
             <p>Some filler text to increase distance.</p>
             <p>More filler text.</p>

             <h1>PART II</h1>
             {} {} {} <p>Changes in accountants content...</p>
             <p>Some other content.</p>

             <h1>PART III</h1>
             <h2>Item 10. Directors and Executive Officers</h2>
             <p>Directors information...</p>
              <p style="page-break-before: always;"></p>
              <p><b>SIGNATURES</b></p>
              <p>Pursuant to the requirements...</p>
              <h2>EXHIBIT INDEX</h2>
         </body>
         </html>
         "#, item8_header, item8_body, item9_header)
     }

     // Helper to create HTML with a ToC structure
     fn create_test_html_with_toc(item8_header_in_toc: &str, item8_header_in_body: &str, item8_body: &str, item9_header_in_body: &str) -> String {
         format!(r##"
 <!DOCTYPE html>
 <html>
 <head><title>Test 10-K Filing with ToC</title></head>
 <body>
     <p>Some introductory text before ToC.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <p>Blah blah blah.</p>
     <h1>Table of Contents</h1>
     <div class="toc">
         <p><a href="#item1">Item 1. Business</a>...........................1</p>
         <p><a href="#item1a">Item 1A. Risk Factors</a>......................10</p>
         <p><a href="#item7">Item 7. MD&A</a>.............................40</p>
         <p><a href="#item8link">{}</a>..................50</p> <p><a href="#item9link">Item 9. Changes</a>.........................55</p>
         <p><a href="#item10link">Item 10. Directors</a>......................60</p>
     </div>
     <hr style="page-break-before: always;"/> <p>Some text between ToC end and Part I.</p>
     <h1>PART I</h1>
     <h2 id="item1">Item 1. Business</h2>
     <p>Business description goes here...</p>
     <p>Lots of content for Part I...</p>
     <p>...</p>
     <p>...</p>

     <h1>PART II</h1>
     <p>Some text before Item 8 header.</p>
     <h2 id="item8">{}</h2> {} <p>Some text before Item 9 header.</p>
     <h2 id="item9">{}</h2> <p>Changes in accountants content...</p>

     <h1>PART III</h1>
     <h2>Item 10. Directors and Executive Officers</h2>
     <p>Directors information...</p>
 </body>
 </html>
 "##, item8_header_in_toc, item8_header_in_body, item8_body, item9_header_in_body)
     }

    // --- Test Cases ---

    // Basic test with standard H2 tags
    #[test]
    fn test_find_item_8_bounds_simple_h2() {
        // Mock financial content
        let item8_body = r#"<h3>Consolidated Balance Sheets</h3><table><tr><td>Assets</td><td>100</td></tr></table><p>Notes to financial statements</p>"#;
        let html = create_test_html(
            "<h2>Item 8. Financial Statements and Supplementary Data</h2>",
            item8_body,
            "<h2>Item 9. Changes in and Disagreements with Accountants</h2>"
        );
        let result = find_item_8_bounds(&html);
        assert!(result.is_some(), "Failed to find bounds for simple H2 case");
        if let Some((start, end)) = result {
            let extracted = &html[start..end];
            assert!(extracted.starts_with("<h2>Item 8."), "Extracted content should start with H2 tag");
            assert!(extracted.contains(item8_body), "Extracted content missing body");
            assert!(extracted.contains("Notes to financial statements"), "Extracted content missing notes phrase");
            assert!(!extracted.contains("Item 9."), "Extracted content should not contain Item 9 header");
            assert!(!extracted.contains("PART III"), "Extracted content should not contain PART III");
        }
    }

    // Test with bold paragraph tags often used
    #[test]
    fn test_find_item_8_bounds_bold_paragraph() {
         let item8_body = r#"<div>Consolidated Statement of Operations</div><div>Revenue: 500</div><p>notes to financial statements</p>"#;
         let html = create_test_html(
             "<p><b>Item 8. Financial Statements and Supplementary Data</b></p>",
             item8_body,
             "<p><b>Item 9. Changes in and Disagreements...</b></p>"
         );
         let result = find_item_8_bounds(&html);
          assert!(result.is_some(), "Failed to find bounds for bold paragraph case");
         if let Some((start, end)) = result {
             let extracted = &html[start..end];
             assert!(extracted.contains("<p><b>Item 8."), "Should contain bold Item 8 paragraph start");
             assert!(extracted.contains(item8_body), "Extracted content missing body");
             assert!(!extracted.contains("Item 9."), "Extracted content should not contain Item 9 marker");
         }
    }

    // Test case where Item 9 marker is absent, should end before PART III or Signatures
    #[test]
    fn test_find_item_8_bounds_no_item9() {
        let item8_body = r#"<h3>Consolidated Balance Sheets</h3><table><tr><td>Assets</td><td>100</td></tr></table><p>Report of Independent Registered Public Accounting Firm</p>"#; // Add audit report phrase
        let html = create_test_html(
            "<h2>Item 8. Financial Statements</h2>", // Slightly different header text
             item8_body,
            "" // No Item 9 header provided in template args
        );

        // Manually find where Item 10 / PART III / Signatures start in the *rendered* test HTML
        let part3_marker = "<h1>PART III</h1>";
        let item10_marker = "<h2>Item 10. Directors";
        let sig_marker = "<b>SIGNATURES</b>";
        let exhibit_marker = "<h2>EXHIBIT INDEX</h2>";

        let part3_pos = html.find(part3_marker).unwrap_or(html.len());
        let item10_pos = html.find(item10_marker).unwrap_or(html.len());
        let sig_pos = html.find(sig_marker).unwrap_or(html.len());
        let exhibit_pos = html.find(exhibit_marker).unwrap_or(html.len());

        // Expected end is the earliest of these markers
        let expected_end = [part3_pos, item10_pos, sig_pos, exhibit_pos].iter().min().copied().unwrap_or(html.len());

        let result = find_item_8_bounds(&html);
        assert!(result.is_some(), "Failed to find bounds when Item 9 is missing");

        if let Some((start, end)) = result {
            let extracted = &html[start..end];
            assert!(extracted.starts_with("<h2>Item 8."), "Should start with H2 tag");
            assert!(extracted.contains(item8_body), "Extracted content missing body");
            assert!(extracted.contains("Accounting Firm"), "Should contain audit report phrase"); // Check validation keyword
            assert!(!extracted.contains(item10_marker), "Extracted content contains Item 10 marker");
            assert!(!extracted.contains(part3_marker), "Extracted content contains PART III marker");
            assert!(!extracted.contains(sig_marker), "Extracted content contains SIGNATURES marker");
            assert!(!extracted.contains(exhibit_marker), "Extracted content contains EXHIBIT INDEX marker");

            // Check if the found end position is close to the expected end position
             assert!(end >= expected_end.saturating_sub(10) && end <= expected_end.saturating_add(10),
                     "End position {} is too far from the earliest expected end marker start {}", end, expected_end);
        }
    }

    // Test to ensure the extractor skips the ToC entry and finds the real one
    #[test]
    fn test_find_item_8_bounds_toc_skip() {
        let item8_body = r#"<h3>Consolidated Balance Sheets</h3><table><tr><td>Assets</td><td>100</td></tr></table><p>Notes to Consolidated Financial Statements</p>"#;
        let html = create_test_html_with_toc(
            "Item 8. Financial Statements", // Header as it appears in ToC
            "Item 8. Financial Statements and Supplementary Data", // Header in main body
            item8_body,
            "Item 9. Changes in and Disagreements with Accountants" // Item 9 in main body
        );

        // Find the positions manually for assertion
        let toc_item8_marker = r#"<a href="#item8link">Item 8. Financial Statements</a>"#;
        let actual_item8_marker = r#"<h2 id="item8">Item 8. Financial Statements and Supplementary Data</h2>"#;

        let toc_item8_pos = html.find(toc_item8_marker).expect("ToC Item 8 marker not found in test HTML");
        let actual_header_pos = html.find(actual_item8_marker).expect("Actual Item 8 marker not found in test HTML");

        println!("Debug Positions: ToC Item 8 at {}, Actual H2 at {}", toc_item8_pos, actual_header_pos); // Add print for debugging

        let result = find_item_8_bounds(&html);
        assert!(result.is_some(), "Should find the main content Item 8 bounds");

        if let Some((start, end)) = result {
            println!("Debug Found Bounds: Start at {}, End at {}", start, end); // Add print for debugging

             // Assert that the found start position is at the actual header, NOT the ToC entry
             // Allow a small buffer in case of slight variations in regex start match
             assert!(start >= actual_header_pos.saturating_sub(5) && start <= actual_header_pos.saturating_add(5),
                    "Start position {} should be at actual header {}, not ToC area starting near {}", start, actual_header_pos, toc_item8_pos);

            let extracted = &html[start..end];
            assert!(extracted.contains(item8_body), "Extracted content missing body");
            assert!(extracted.contains("Notes to Consolidated"), "Extracted content missing validation phrase");
            assert!(!extracted.contains("Item 10."), "Extracted content should not contain Item 10");
             assert!(!extracted.contains(r#"<div class="toc">"#), "Extracted content should not contain ToC div"); // Ensure we didn't grab ToC
        }
    }

    // Test the full extractor logic integration
    #[test]
    fn test_extractor_integration_basic() {
        let item8_body = r#"<h3>Consolidated Balance Sheets</h3><table><tr><td>Assets</td><td>100</td></tr></table><p>Some notes here.</p><p>Consolidated Statement of Cash Flows</p>"#; // Add validation keyword
        let html = create_test_html(
            "<h2>Item 8. Financial Statements and Supplementary Data</h2>",
             item8_body,
            "<h2>Item 9. Changes in and Disagreements with Accountants</h2>"
        );
        let extractor = SectionExtractor::new();
        let result = extractor.extract_item_8(&html, 2023, "TestCo", "TST", TEST_MIN_SIZE);
        assert!(result.is_ok(), "Extractor failed on basic case: {:?}", result.err());
        if let Ok(section) = result {
            assert!(section.content.contains("Consolidated Balance Sheets"));
             assert!(section.content.len() > TEST_MIN_SIZE);
             assert!(!section.content.contains("Item 9."));
        }
    }

    // Test integration with minimum size constraint failing
    #[test]
    fn test_extractor_integration_too_small() {
        // Content has financial keywords but is very short
        let item8_body = r#"<p>Consolidated Balance Sheet</p>Assets: 10."#;
        let html = create_test_html(
            "<h2>Item 8. Financial Statements</h2>",
             item8_body,
            "<h2>Item 9. Changes</h2>"
        );
        let extractor = SectionExtractor::new();
        // Set min size much larger than the content
        let result = extractor.extract_item_8(&html, 2023, "TestCo", "TST", 500);
        assert!(result.is_err(), "Extractor should fail due to min size constraint");
        match result.err().unwrap() {
            ExtractError::SectionNotFound(msg) => {
                assert!(msg.contains("less than minimum"), "Unexpected error message for size failure: {}", msg);
            },
            e => panic!("Expected SectionNotFound error due to size, got {:?}", e),
        }
    }

    // Test integration where the content lacks financial keywords for validation
     #[test]
     fn test_extractor_integration_no_financial_content() {
         // Content is reasonably long but lacks keywords
         let item8_body = r#"<p>See notes elsewhere.</p><p>This section provides details.</p><p>Filler text to make it longer than min size.</p><p>More filler.</p><p>Even more filler.</p>"#;
         let html = create_test_html(
             "<h2>Item 8. Financial Statements</h2>",
              item8_body,
             "<h2>Item 9. Changes</h2>"
         );
         let extractor = SectionExtractor::new();
         let result = extractor.extract_item_8(&html, 2023, "TestCo", "TST", TEST_MIN_SIZE);
          assert!(result.is_err(), "Extractor should fail due to lack of financial content in preview/final check");
          match result.err().unwrap() {
              ExtractError::SectionNotFound(msg) => {
                  // Expect error related to validation failure or no valid start found
                   assert!(msg.contains("No validated Item 8 start marker found") || msg.contains("failed final content validation"), "Unexpected error message for content validation failure: {}", msg);
              },
              e => panic!("Expected SectionNotFound error due to content validation, got {:?}", e),
          }
     }

     // Test the is_in_table_of_contents helper directly
     #[test]
     fn test_is_in_table_of_contents_logic() {
         // Simulate HTML segments
          let toc_html_simple = r##"<body><h1>Table of Contents</h1><div class="toc"><p><a href="#i8">Item 8. FS</a>...50</p></div><hr/><p>Real content starts</p></body>"##;
          let toc_html_nav = r##"<body><nav class="tableofcontents"><ul><li>Item 1</li><li>Item 8 Financials</li></ul></nav><p>Real content starts</p></body>"##;
          let content_html = r##"<body>...<hr/><h2>PART II</h2><h2 id="item8">Item 8. FS</h2><p>Consolidated...</p></body>"##;
         let early_content_html = r##"<body><p>Intro text.</p><h2>Item 8 Details</h2><p>Not financials.</p></body>"##; // Item 8 mentioned early, but not ToC/financials

         // Find positions within the simulated segments
         let toc_match_pos_simple = toc_html_simple.find("Item 8. FS").unwrap();
         let after_toc_end_marker_pos = toc_html_simple.find("Real content starts").unwrap();

         let toc_match_pos_nav = toc_html_nav.find("Item 8 Financials").unwrap();
         let after_nav_end_pos = toc_html_nav.find("Real content starts").unwrap();

         let content_match_pos = content_html.find("Item 8. FS").unwrap();
         let early_match_pos = early_content_html.find("Item 8 Details").unwrap();


         // Assertions
         assert!(is_in_table_of_contents(toc_html_simple, toc_match_pos_simple), "Should detect pos within ToC div");
         assert!(!is_in_table_of_contents(toc_html_simple, after_toc_end_marker_pos), "Should NOT detect pos after HR end marker as ToC");

         assert!(is_in_table_of_contents(toc_html_nav, toc_match_pos_nav), "Should detect pos within ToC nav");
         assert!(!is_in_table_of_contents(toc_html_nav, after_nav_end_pos), "Should NOT detect pos after nav end marker as ToC");

         assert!(!is_in_table_of_contents(content_html, content_match_pos), "Should NOT detect pos in main content (after HR) as ToC");

         // Test early position check - this should be caught by positional/early checks now
          assert!(is_in_table_of_contents(early_content_html, early_match_pos), "Should likely detect very early position as ToC/intro even without explicit markers");
     }
}