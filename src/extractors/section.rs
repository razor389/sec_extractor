// src/extractors/section.rs
use crate::utils::error::ExtractError;
use regex::Regex;
use std::collections::HashMap;
use std::env;

/// Returns the configured minimum section size.
/// Reads from MIN_SECTION_SIZE environment variable, defaulting to 50 
/// for tests if not set.
fn get_min_section_size() -> usize {
    match env::var("MIN_SECTION_SIZE") {
        Ok(val) => val.parse().unwrap_or(50),
        Err(_) => 50, // Default value for tests
    }
}

/// Checks if a position is within a table of contents section
fn is_in_table_of_contents(html_content: &str, position: usize) -> bool {
    // Common ToC indicators
    let toc_indicators = [
        r"(?i)<h[1-6][^>]*>\s*(?:table\s+of\s+contents|index|contents)\s*</h[1-6]>",
        // Changed to use hash-delimited raw string literal
        r#"(?i)<div[^>]*class=['"]?(?:toc|tableOfContents|index)['"]?[^>]*>"#,
        r"(?i)table\s+of\s+contents",
    ];
    
    // Look for ToC indicators before the position
    // Check within a reasonable range (e.g., 5000 characters) before the position
    let start_search = if position > 5000 { position - 5000 } else { 0 };
    let content_before = &html_content[start_search..position];
    
    for pattern in &toc_indicators {
        if let Ok(re) = Regex::new(pattern) {
            if re.is_match(content_before) {
                // Now check if we're still in the ToC section
                // Look for common end-of-ToC markers
                let end_toc_markers = [
                    r"(?i)</div>\s*<h[1-6]", // End of div followed by a heading
                    r"(?i)</nav>",           // End of navigation section
                    r"(?i)<hr",              // Horizontal rule often separates ToC
                    r"(?i)<h[1-6][^>]*>\s*PART\s+I\s*</h[1-6]>", // Start of Part I
                ];
                
                for end_pattern in &end_toc_markers {
                    if let Ok(end_re) = Regex::new(end_pattern) {
                        if let Some(end_match) = end_re.find(content_before) {
                            // If we found an end-of-ToC marker before our position,
                            // then we're not in the ToC
                            if start_search + end_match.end() < position {
                                return false;
                            }
                        }
                    }
                }
                
                // If we found a ToC indicator but no end marker, we're likely in a ToC
                return true;
            }
        }
    }
    
    false
}

#[allow(dead_code)]
// Helper function to find the end of a section starting from a position
fn find_section_end(html_content: &str, start_pos: usize) -> Option<(usize, usize)> {
    // Look for Item 9 or PART III to determine the end
    let end_patterns = [
        r"(?i)<h[1-6][^>]*>\s*Item\s*9\.?\s*Changes\s*in\s*and\s*Disagreements\s*with\s*Accountants\s*</h[1-6]>",
        r"(?i)Item\s*9\.?\s*Changes\s*in\s*and\s*Disagreements\s*with\s*Accountants",
        r"(?i)<a[^>]*>\s*Item\s*9\.\s*</a>\s*Changes\s*in\s*and\s*Disagreements\s*with\s*Accountants",
        r"(?i)Item\s*9[\.\s\-–—:]+\s*Changes\s*in\s*and\s*Disagreements\s*with\s*Accountants",
        r"(?i)item\s*9[\.\s]*",
        r"(?i)item\s*9[\.\s]*\(?changes",
        r"(?i)<h[1-6][^>]*>\s*PART\s*III\s*</h[1-6]>"
    ];
    
    let search_from = start_pos + 100; // Skip a bit to avoid early matches
    
    let mut end_pos = None;
    for pattern in &end_patterns {
        if let Ok(re) = Regex::new(pattern) {
            if let Some(mat) = re.find(&html_content[search_from..]) {
                end_pos = Some(search_from + mat.start());
                break;
            }
        }
    }
    
    // If no end marker found, use a reasonable chunk size
    let end_pos = end_pos.unwrap_or_else(|| (start_pos + 300000).min(html_content.len()));
    
    // Validate that we have a reasonable section size
    let min_section_size = get_min_section_size();
    if end_pos > start_pos && end_pos - start_pos > min_section_size {
        // Validate that we have actual financial content, not just a section heading
        let content = &html_content[start_pos..end_pos];
        let content_lower = content.to_lowercase();
        
        let has_financial_terms = 
            content_lower.contains("consolidated balance sheet") ||
            content_lower.contains("statement of operations") ||
            content_lower.contains("statement of income") ||
            content_lower.contains("statement of cash flow") ||
            content_lower.contains("notes to consolidated") ||
            (content_lower.contains("report") && 
             content_lower.contains("independent") && 
             content_lower.contains("audit"));
            
        let has_financial_tables = 
            content.contains("<table") && 
            (content_lower.contains("assets") || 
             content_lower.contains("liabilities") ||
             content_lower.contains("equity") ||
             content_lower.contains("revenue") ||
             content_lower.contains("expense"));
        
        if has_financial_terms || has_financial_tables {
            Some((start_pos, end_pos))
        } else {
            None
        }
    } else {
        None
    }
}

/// Represents an extracted section from a 10-K filing
#[derive(Debug, Clone)]
pub struct ExtractedSection {
    pub section_name: String,     // e.g., "Item 8"
    pub section_title: String,    // e.g., "Financial Statements and Supplementary Data"
    pub content: String,          // The raw HTML/text content
    pub filing_year: u32,         // The year of the filing
    pub company_name: String,     // Company name
    pub ticker: String,           // Ticker symbol
}

/// Extraction Strategy trait for implementing different section extraction methods
pub trait ExtractionStrategy {
    fn name(&self) -> &'static str;
    fn extract(&self, html_content: &str) -> Option<(usize, usize)>;
}

/// Pattern-based extraction strategy using regular expressions
pub struct PatternExtractionStrategy {
    pub name: &'static str,
    pub start_patterns: Vec<&'static str>,
    pub end_patterns: Vec<&'static str>,
}

impl ExtractionStrategy for PatternExtractionStrategy {
    fn name(&self) -> &'static str {
        self.name
    }
    
    fn extract(&self, html_content: &str) -> Option<(usize, usize)> {
        // Find start position
        let mut start_pos = None;
        for pattern in &self.start_patterns {
            if let Ok(re) = Regex::new(pattern) {
                // Find all occurrences
                for mat in re.find_iter(html_content) {
                    // Skip matches in table of contents
                    if is_in_table_of_contents(html_content, mat.start()) {
                        tracing::debug!("Skipping ToC match for pattern '{}' at position: {}", 
                            pattern, mat.start());
                        continue;
                    }
                    
                    // Verify this looks like the actual Item 8 section, not just a reference
                    // Check that there's substantial content following it
                    let content_check_end = (mat.end() + 2000).min(html_content.len());
                    let following_content = &html_content[mat.end()..content_check_end];
                    
                    // Look for indicators of actual financial content
                    let has_financial_terms = 
                        following_content.to_lowercase().contains("consolidated") ||
                        following_content.to_lowercase().contains("balance sheet") ||
                        following_content.to_lowercase().contains("income statement") ||
                        following_content.to_lowercase().contains("cash flow") ||
                        following_content.to_lowercase().contains("financial statement") ||
                        following_content.to_lowercase().contains("audit") ||
                        following_content.to_lowercase().contains("notes to");
                    
                    if has_financial_terms {
                        start_pos = Some(mat.start());
                        tracing::debug!("Found valid start pattern match: '{}' at position: {}", 
                            pattern, mat.start());
                        break;
                    } else {
                        tracing::debug!("Skipping match without financial content at position: {}", 
                            mat.start());
                    }
                }
                
                if start_pos.is_some() {
                    break;
                }
            }
        }
        
        let start_pos = start_pos?;
        
        // Find end position
        let mut end_pos = None;
        for pattern in &self.end_patterns {
            if let Ok(re) = Regex::new(pattern) {
                // Search from start_pos to the end of the document
                if let Some(mat) = re.find(&html_content[start_pos + 100..]) {
                    end_pos = Some(start_pos + 100 + mat.start());
                    tracing::debug!("Found end pattern match: '{}' at position: {}", pattern, end_pos.unwrap());
                    break;
                }
            }
        }
        
        // If no end marker found, try looking for the next Item heading
        if end_pos.is_none() {
            // Look for any Item X heading after Item 8
            let next_item_re = Regex::new(r"(?i)<h[1-6][^>]*>\s*Item\s*[0-9]+\.?").ok();
            if let Some(re) = next_item_re {
                if let Some(mat) = re.find(&html_content[start_pos + 1000..]) { // Skip a bit to avoid re-matching Item 8
                    end_pos = Some(start_pos + 1000 + mat.start());
                    tracing::debug!("Found next Item heading at position: {}", end_pos.unwrap());
                }
            }
        }
        
        // If still no end marker found, use a larger chunk size or go to the end
        let end_pos = end_pos.unwrap_or_else(|| (start_pos + 300000).min(html_content.len()));
        
        // Only return if the section is reasonably sized
        if end_pos > start_pos && end_pos - start_pos > get_min_section_size() {
            Some((start_pos, end_pos))
        } else {
            None
        }
    }
}

/// Table of Contents (ToC) based extraction strategy
pub struct TocExtractionStrategy;

impl ExtractionStrategy for TocExtractionStrategy {
    fn name(&self) -> &'static str {
        "ToC Strategy"
    }
    
    fn extract(&self, html_content: &str) -> Option<(usize, usize)> {
        // Look for ToC links to Item 8
        let toc_re = Regex::new(r#"(?i)<a[^>]*href="[^"]*(?:item[_\-]?8|financial[_\-]statements)[^"]*"[^>]*>.*?item\s*8.*?</a>"#).ok()?;
        
        if let Some(mat) = toc_re.find(html_content) {
            // Extract the href attribute
            let href_re = Regex::new(r#"href="([^"]*)""#).ok()?;
            if let Some(href_mat) = href_re.captures(&html_content[mat.start()..mat.end()]) {
                if let Some(href) = href_mat.get(1) {
                    let href_val = href.as_str();
                    tracing::debug!("Found ToC link: {}", href_val);
                    
                    // Find the target anchor in the document
                    let anchor_pattern = if href_val.starts_with("#") {
                        format!(r#"(?i)<[^>]*(?:id|name)="{}"[^>]*>"#, &href_val[1..])
                    } else {
                        format!(r#"(?i)<[^>]*(?:id|name)="[^"]*{}"[^>]*>"#, href_val)
                    };
                    
                    let anchor_re = Regex::new(&anchor_pattern).ok()?;
                    if let Some(anchor_mat) = anchor_re.find(html_content) {
                        let start_pos = anchor_mat.start();
                        
                        // Look for Item 9 or PART III to determine the end
                        let end_patterns = [
                            r"(?i)<h[1-6][^>]*>\s*Item\s*9\.?\s*",
                            r"(?i)Item\s*9[\.\s]*\(?changes",
                            r"(?i)<h[1-6][^>]*>\s*PART\s*III\s*</h[1-6]>"
                        ];
                        
                        let mut end_pos = None;
                        for pattern in &end_patterns {
                            if let Ok(re) = Regex::new(pattern) {
                                if let Some(mat) = re.find(&html_content[start_pos..]) {
                                    end_pos = Some(start_pos + mat.start());
                                    break;
                                }
                            }
                        }
                        
                        // If no end marker found, use a reasonable chunk size
                        let end_pos = end_pos.unwrap_or_else(|| (start_pos + 200000).min(html_content.len()));
                        
                        return Some((start_pos, end_pos));
                    }
                }
            }
        }
        
        None
    }
}

/// Specialized strategy to find the actual Item 8 content, not ToC references
pub struct ActualItem8ExtractionStrategy;

impl ExtractionStrategy for ActualItem8ExtractionStrategy {
    fn name(&self) -> &'static str {
        "Actual Item 8 Content Strategy"
    }
    
    fn extract(&self, html_content: &str) -> Option<(usize, usize)> {
        // First find PART II which typically contains Item 8
        let part2_re = Regex::new(r"(?i)<h[1-6][^>]*>\s*PART\s*II\s*</h[1-6]>").ok()?;
        let part2_matches: Vec<_> = part2_re.find_iter(html_content).collect();
        
        // If we can't find PART II, try the whole document
        let search_from = if !part2_matches.is_empty() {
            part2_matches[0].start()
        } else {
            0
        };
        
        // Search for Item 8 heading after PART II and record header end.
        let item8_patterns = [
            r"(?i)<h[1-6][^>]*>\s*Item\s*8\.?\s*Financial\s*Statements\s*and\s*Supplementary\s*Data\s*</h[1-6]>",
            r"(?i)<div[^>]*>\s*(?:<strong>)?\s*Item\s*8\.?\s*Financial\s*Statements\s*and\s*Supplementary\s*Data\s*(?:</strong>)?\s*</div>",
            r"(?i)<p[^>]*>\s*(?:<strong>)?\s*Item\s*8\.?\s*Financial\s*Statements\s*and\s*Supplementary\s*Data\s*(?:</strong>)?\s*</p>",
            r"(?i)<span[^>]*>\s*Item\s*8\.?\s*Financial\s*Statements\s*and\s*Supplementary\s*Data\s*</span>",
            r"(?i)<font[^>]*>\s*Item\s*8\.?\s*Financial\s*Statements\s*and\s*Supplementary\s*Data\s*</font>",
            r"(?i)Item\s*8[\.\s\-–—:]+\s*Financial\s*Statements\s*and\s*Supplementary\s*Data"
        ];
        
        let mut start_pos = None;
        let mut header_end = None; // We'll record where the header ends.
        
        for pattern in &item8_patterns {
            if let Ok(re) = Regex::new(pattern) {
                // Search from our determined offset
                for mat in re.find_iter(&html_content[search_from..]) {
                    let actual_pos = search_from + mat.start();
                    
                    // Skip if this is in a table of contents.
                    if is_in_table_of_contents(html_content, actual_pos) {
                        tracing::debug!("Skipping ToC match at position: {}", actual_pos);
                        continue;
                    }
                    
                    // Use the end of the matched header as our new search offset.
                    let candidate_header_end = search_from + mat.end();
                    let look_ahead = 5000;
                    let end_preview = (candidate_header_end + look_ahead).min(html_content.len());
                    let preview = &html_content[candidate_header_end..end_preview];
                    
                    // Verify there are financial content indicators.
                    if preview.to_lowercase().contains("consolidated")
                        || preview.to_lowercase().contains("balance sheet")
                        || preview.to_lowercase().contains("statement of")
                        || preview.to_lowercase().contains("cash flow")
                        || preview.to_lowercase().contains("report of independent")
                        || (preview.to_lowercase().contains("opinion") && preview.to_lowercase().contains("audit"))
                    {
                        start_pos = Some(actual_pos);
                        header_end = Some(candidate_header_end);
                        tracing::info!("Found actual Item 8 content at position {}", actual_pos);
                        break;
                    }
                }
                if start_pos.is_some() {
                    break;
                }
            }
        }
        
        // If still not found, try broader patterns...
        if start_pos.is_none() {
            let broader_patterns = [
                r"(?i)item\s*8[\.\s]*",
                r"(?i)financial\s+statements\s+and\s+supplementary\s+data"
            ];
            
            for pattern in &broader_patterns {
                if let Ok(re) = Regex::new(pattern) {
                    for mat in re.find_iter(&html_content[search_from..]) {
                        let actual_pos = search_from + mat.start();
                        
                        if is_in_table_of_contents(html_content, actual_pos) {
                            continue;
                        }
                        
                        let candidate_header_end = search_from + mat.end();
                        let look_ahead = 10000; // Look further with broader patterns
                        let end_preview = (candidate_header_end + look_ahead).min(html_content.len());
                        let preview = &html_content[candidate_header_end..end_preview];
                        
                        if (preview.to_lowercase().contains("consolidated balance sheet") ||
                            preview.to_lowercase().contains("statement of operations") ||
                            preview.to_lowercase().contains("statement of income") ||
                            preview.to_lowercase().contains("statement of cash flow"))
                           && (preview.contains("<table") || preview.to_lowercase().contains("notes to"))
                        {
                            start_pos = Some(actual_pos);
                            header_end = Some(candidate_header_end);
                            tracing::info!("Found actual Item 8 with broader pattern at position {}", actual_pos);
                            break;
                        }
                    }
                    if start_pos.is_some() {
                        break;
                    }
                }
            }
        }
        
        // If we found a valid start position, now search for an end marker.
        if let Some(start) = start_pos {
            // Use the end of the header as the starting point; if not available, use start + a small offset.
            let search_offset = header_end.unwrap_or((start + 100).min(html_content.len()));
            
            // Modified end patterns: match any header that begins with "Item 9. Changes"
            let end_patterns = [
                r"(?i)<h[1-6][^>]*>\s*Item\s*9\.?\s*Changes\b",
                r"(?i)<h[1-6][^>]*>\s*PART\s*III\s*</h[1-6]>",
                r"(?i)<h[1-6][^>]*>\s*Item\s*10\.?\s*Directors",
                r"(?i)PART\s*III",
            ];
            
            let mut end_pos = None;
            for pattern in &end_patterns {
                if let Ok(re) = Regex::new(pattern) {
                    if let Some(mat) = re.find(&html_content[search_offset..]) {
                        end_pos = Some(search_offset + mat.start());
                        tracing::debug!("Found end pattern match at position: {}", end_pos.unwrap());
                        break;
                    }
                }
            }
            
            // If no end marker was found, use a fallback that goes to the end of the document.
            let end_pos = end_pos.unwrap_or_else(|| (start + 300000).min(html_content.len()));
            let min_section_size = get_min_section_size();
            if end_pos > start && end_pos - start > min_section_size {
                return Some((start, end_pos));
            }
        }
        
        None
    }
}

/// Financial Statement extraction strategy
/// This looks for common financial statement headings that would be in Item 8
pub struct FinancialStatementExtractionStrategy;

impl ExtractionStrategy for FinancialStatementExtractionStrategy {
    fn name(&self) -> &'static str {
        "Financial Statement Strategy"
    }
    
    fn extract(&self, html_content: &str) -> Option<(usize, usize)> {
        // Patterns for common financial statement headings
        let statement_patterns = [
            r"(?i)<h[1-6][^>]*>\s*consolidated\s+financial\s+statements\s*</h[1-6]>",
            r"(?i)<h[1-6][^>]*>\s*consolidated\s+statements\s+of\s+operations\s*</h[1-6]>",
            r"(?i)<h[1-6][^>]*>\s*consolidated\s+statements\s+of\s+income\s*</h[1-6]>",
            r"(?i)<h[1-6][^>]*>\s*consolidated\s+balance\s+sheets?\s*</h[1-6]>",
            r"(?i)<h[1-6][^>]*>\s*consolidated\s+statements\s+of\s+cash\s+flows?\s*</h[1-6]>",
        ];
        
        let mut best_match: Option<regex::Match> = None;
        for pattern in &statement_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(mat) = re.find(html_content) {
                    // Skip if this is in a table of contents
                    if is_in_table_of_contents(html_content, mat.start()) {
                        continue;
                    }
                    
                    best_match = match best_match {
                        Some(current) => {
                            if mat.start() < current.start() {
                                Some(mat)
                            } else {
                                Some(current)
                            }
                        },
                        None => Some(mat),
                    };
                }
            }
        }
        
        let start_pos = best_match.map(|m| m.start())?;
        
        // Look for the end marker (either Part III or a fallback based on a chunk size)
        let end_patterns = [
            r"(?i)<h[1-6][^>]*>\s*PART\s*III\s*</h[1-6]>",
            r"(?i)<h[1-6][^>]*>\s*Item\s*9[\.\s]*"
        ];
        
        let mut end_pos = None;
        for pattern in &end_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(mat) = re.find(&html_content[start_pos..]) {
                    let pos = start_pos + mat.start();
                    let min_section_size = get_min_section_size();
                    if pos > start_pos + min_section_size {
                        end_pos = Some(pos);
                        break;
                    }
                }
            }
        }
        
        let end_pos = end_pos.unwrap_or_else(|| (start_pos + 200000).min(html_content.len()));
        
        Some((start_pos, end_pos))
    }
}

/// Part II-based extraction strategy
/// Looks for Item 8 after the PART II heading
pub struct PartIIExtractionStrategy;

impl ExtractionStrategy for PartIIExtractionStrategy {
    fn name(&self) -> &'static str {
        "Part II Strategy"
    }
    
    fn extract(&self, html_content: &str) -> Option<(usize, usize)> {
        // Find the PART II heading
        let part2_re = Regex::new(r"(?i)<h[1-6][^>]*>\s*PART\s*II\s*</h[1-6]>").ok()?;
        let part2_mat = part2_re.find(html_content)?;
        let part2_pos = part2_mat.start();
        
        // Look for Item 8 after PART II
        let item8_re = Regex::new(r"(?i)item\s*8\.?").ok()?;
        let search_from = part2_pos;
        let item8_matches: Vec<_> = item8_re.find_iter(&html_content[search_from..]).collect();
        
        if item8_matches.is_empty() {
            return None;
        }
        
        // Find the first Item 8 reference that isn't in the ToC
        for item8_mat in item8_matches {
            let start_pos = search_from + item8_mat.start();
            
            // Skip if this is in a table of contents
            if is_in_table_of_contents(html_content, start_pos) {
                continue;
            }
            
            // Look ahead for financial content indicators
            let look_ahead = 5000;
            let end_preview = (start_pos + item8_mat.len() + look_ahead).min(html_content.len());
            let preview = &html_content[start_pos + item8_mat.len()..end_preview];
            
            // Check for financial statement indicators
            if preview.to_lowercase().contains("financial statements") || 
               preview.to_lowercase().contains("balance sheet") ||
               preview.to_lowercase().contains("statement of") ||
               preview.to_lowercase().contains("cash flow") ||
               preview.to_lowercase().contains("report of independent") ||
               (preview.to_lowercase().contains("opinion") && preview.to_lowercase().contains("audit")) {
                
                // Look for Item 9 or PART III to determine the end
                let end_patterns = [
                    r"(?i)<h[1-6][^>]*>\s*Item\s*9\.?\s*",
                    r"(?i)Item\s*9[\.\s]*\(?changes",
                    r"(?i)<h[1-6][^>]*>\s*PART\s*III\s*</h[1-6]>"
                ];
                
                let mut end_pos = None;
                let search_from_end = (start_pos + 1000).min(html_content.len()); // Clamp to length
                for pattern in &end_patterns {
                    if let Ok(re) = Regex::new(pattern) {
                        if let Some(mat) = re.find(&html_content[search_from_end..]) {
                            end_pos = Some(search_from_end + mat.start());
                            break;
                        }
                    }
                }

                // If no end marker found, use a reasonable chunk size
                let end_pos = end_pos.unwrap_or_else(|| (start_pos + 300000).min(html_content.len()));
                
                // Only return a result if the section is reasonably sized
                let min_section_size = get_min_section_size();
                if end_pos > start_pos && end_pos - start_pos > min_section_size {
                    return Some((start_pos, end_pos));
                }
            }
        }
        
        None
    }
}

/// Section Extractor that uses multiple strategies to find the desired section
pub struct SectionExtractor {
    strategies: Vec<Box<dyn ExtractionStrategy>>,
}

impl SectionExtractor {
    /// Creates a new SectionExtractor with default extraction strategies
    pub fn new() -> Self {
        // Define standard patterns for Item 8 section extraction
        let standard_strategy = PatternExtractionStrategy {
            name: "Standard Item 8 Strategy",
            start_patterns: vec![
                r"(?i)<h[1-6][^>]*>\s*Item\s*8\.?\s*Financial\s*Statements\s*and\s*Supplementary\s*Data\s*</h[1-6]>",
                r"(?i)Item\s*8\.?\s*Financial\s*Statements\s*and\s*Supplementary\s*Data",
                r"(?i)<a[^>]*>\s*Item\s*8\.\s*</a>\s*Financial\s*Statements\s*and\s*Supplementary\s*Data",
                r"(?i)Item\s*8[\.\s\-–—:]+\s*Financial\s*Statements\s*and\s*Supplementary\s*Data",
                r"(?i)item\s*8[\.\s]*\(?financial\s+statements\s+and\s+supplementary\s+data\)?",
                r"(?i)item\s*8[\.\s]*"
            ],
            end_patterns: vec![
                r"(?i)<h[1-6][^>]*>\s*Item\s*9\.?\s*Changes\s*in\s*and\s*Disagreements\s*with\s*Accountants\s*</h[1-6]>",
                r"(?i)Item\s*9\.?\s*Changes\s*in\s*and\s*Disagreements\s*with\s*Accountants",
                r"(?i)<a[^>]*>\s*Item\s*9\.\s*</a>\s*Changes\s*in\s*and\s*Disagreements\s*with\s*Accountants",
                r"(?i)Item\s*9[\.\s\-–—:]+\s*Changes\s*in\s*and\s*Disagreements\s*with\s*Accountants",
                r"(?i)item\s*9[\.\s]*",
                r"(?i)item\s*9[\.\s]*\(?changes",
                r"(?i)<h[1-6][^>]*>\s*PART\s*III\s*</h[1-6]>"
            ]
        };
        
        // Create a vector of strategies to try in order
        let strategies: Vec<Box<dyn ExtractionStrategy>> = vec![
            Box::new(TocExtractionStrategy), // Our new ToC-guided strategy first
            Box::new(ActualItem8ExtractionStrategy), // Then the strategy to find actual Item 8 content
            Box::new(standard_strategy),
            Box::new(TocExtractionStrategy),
            Box::new(PartIIExtractionStrategy),
            Box::new(FinancialStatementExtractionStrategy),
        ];
        
        Self { strategies }
    }
    
    /// Extracts Item 8 content using multiple strategies
    pub fn extract_item_8(
        &self,
        html_content: &str, 
        filing_year: u32, 
        company_name: &str, 
        ticker: &str
    ) -> Result<ExtractedSection, ExtractError> {
        // Keep track of extraction attempts for debugging
        let mut debug_info = HashMap::new();
        let min_section_size = get_min_section_size();

        // Try each strategy in turn
        for strategy in &self.strategies {
            let strategy_name = strategy.name();
            tracing::debug!("Trying extraction strategy: {}", strategy_name);
            
            if let Some((start_pos, end_pos)) = strategy.extract(html_content) {
                // Validate that we have a reasonable section size
                if end_pos <= start_pos || end_pos - start_pos < min_section_size {
                    debug_info.insert(
                        strategy_name.to_string(), 
                        format!("Section too small: {} bytes", end_pos - start_pos)
                    );
                    continue;
                }
                
                // Extract the content between the markers
                let content = html_content[start_pos..end_pos].to_string();
                
                // Validate we have actual financial content, not just a reference
                let content_lower = content.to_lowercase();
                let has_financial_terms = 
                    content_lower.contains("consolidated balance sheet") ||
                    content_lower.contains("statement of operations") ||
                    content_lower.contains("statement of income") ||
                    content_lower.contains("statement of cash flow") ||
                    content_lower.contains("notes to consolidated") ||
                    (content_lower.contains("report") && 
                     content_lower.contains("independent") && 
                     content_lower.contains("audit"));
                    
                let has_financial_tables = 
                    content.contains("<table") && 
                    (content_lower.contains("assets") || 
                     content_lower.contains("liabilities") ||
                     content_lower.contains("equity") ||
                     content_lower.contains("revenue") ||
                     content_lower.contains("expense"));
                    
                if !has_financial_terms && !has_financial_tables {
                    tracing::warn!(
                        "Strategy '{}' found Item 8 section but it may not contain financial statements!",
                        strategy_name
                    );
                    debug_info.insert(
                        strategy_name.to_string(), 
                        "Found section lacks financial content indicators".to_string()
                    );
                    continue; // Try next strategy instead
                }
                
                tracing::info!(
                    "Successfully extracted Item 8 content using strategy '{}': {} bytes, start: {}, end: {}",
                    strategy_name, content.len(), start_pos, end_pos
                );
                
                return Ok(ExtractedSection {
                    section_name: "Item 8".to_string(),
                    section_title: "Financial Statements and Supplementary Data".to_string(),
                    content,
                    filing_year,
                    company_name: company_name.to_string(),
                    ticker: ticker.to_string(),
                });
            } else {
                debug_info.insert(strategy_name.to_string(), "No match found".to_string());
            }
        }
        
        // If we get here, all strategies failed
        let mut failure_info = String::new();
        for (strategy, reason) in debug_info.iter() {
            failure_info.push_str(&format!("{}: {}\n", strategy, reason));
        }
        
        tracing::debug!("Item 8 extraction failed. Strategy results:\n{}", failure_info);
        Err(ExtractError::SectionNotFound("Item 8".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_env() {
        // Set minimum section size to 50 for tests
        env::set_var("MIN_SECTION_SIZE", "50");
    }
    
    fn create_test_html(item8_content: &str, item9_content: &str) -> String {
        format!(r#"
        <!DOCTYPE html>
        <html>
        <head><title>Test 10-K Filing</title></head>
        <body>
            <h1>PART I</h1>
            <h2>Item 1. Business</h2>
            <p>Business description goes here...</p>
            
            <h1>PART II</h1>
            <h2>{}</h2>
            <p>Financial statements content...</p>
            {}
            
            <h2>{}</h2>
            <p>Changes in accountants content...</p>
            
            <h1>PART III</h1>
            <h2>Item 10. Directors and Executive Officers</h2>
            <p>Directors information...</p>
        </body>
        </html>
        "#, item8_content, "Financial tables go here...", item9_content)
    }
    
    fn create_test_html_with_toc(item8_content: &str, item9_content: &str) -> String {
        format!(r##"
    <!DOCTYPE html>
    <html>
    <head><title>Test 10-K Filing</title></head>
    <body>
        <h1>Table of Contents</h1>
        <div class="toc">
            <a href="#item1">Item 1. Business</a><br>
            <a href="#item8">Item 8. Financial Statements and Supplementary Data</a><br>
            <a href="#item9">Item 9. Changes in and Disagreements with Accountants</a><br>
        </div>
        
        <h1>PART I</h1>
        <h2 id="item1">Item 1. Business</h2>
        <p>Business description goes here...</p>
        
        <h1>PART II</h1>
        <h2 id="item8">{}</h2>
        <p>Financial statements content...</p>
        <h3>Consolidated Balance Sheets</h3>
        <table>
            <tr><th>Assets</th><th>2023</th><th>2022</th></tr>
            <tr><td>Cash</td><td>1000</td><td>800</td></tr>
        </table>
        
        <h2 id="item9">{}</h2>
        <p>Changes in accountants content...</p>
        
        <h1>PART III</h1>
        <h2>Item 10. Directors and Executive Officers</h2>
        <p>Directors information...</p>
    </body>
    </html>
    "##, item8_content, item9_content)
    }    
    
    #[test]
    fn test_standard_strategy() {
        setup_test_env();
        let html = create_test_html(
            "Item 8. Financial Statements and Supplementary Data", 
            "Item 9. Changes in and Disagreements with Accountants"
        );
        
        let strategy = PatternExtractionStrategy {
            name: "Test Strategy",
            start_patterns: vec![r"(?i)Item\s*8\.?\s*Financial\s*Statements"],
            end_patterns: vec![r"(?i)Item\s*9\.?\s*Changes"],
        };
        
        let result = strategy.extract(&html);
        assert!(result.is_some());
        
        let (start, end) = result.unwrap();
        let extracted = &html[start..end];
        assert!(extracted.contains("Item 8. Financial Statements"));
        assert!(!extracted.contains("Item 9. Changes"));
    }
    
    #[test]
    fn test_part_ii_strategy() {
        setup_test_env();
        let html = create_test_html(
            "Item 8. Financial Statements and Supplementary Data", 
            "Item 9. Changes in and Disagreements with Accountants"
        );
        
        let strategy = PartIIExtractionStrategy;
        let result = strategy.extract(&html);
        assert!(result.is_some());
    }
    
    #[test]
    fn test_financial_statement_strategy() {
        setup_test_env();
        let html = r#"
        <!DOCTYPE html>
        <html>
        <body>
            <h1>PART II</h1>
            <h2>Consolidated Financial Statements</h2>
            <h3>Consolidated Balance Sheets</h3>
            <table>
                <tr><th>Assets</th><th>2023</th><th>2022</th></tr>
                <tr><td>Cash</td><td>1000</td><td>800</td></tr>
            </table>
            
            <h3>Consolidated Statements of Operations</h3>
            <table>
                <tr><th>Revenue</th><th>2023</th><th>2022</th></tr>
                <tr><td>Total</td><td>5000</td><td>4500</td></tr>
            </table>
            
            <h2>Item 9. Changes in Accountants</h2>
        </body>
        </html>
        "#;
        
        let strategy = FinancialStatementExtractionStrategy;
        let result = strategy.extract(html);
        assert!(result.is_some());
        
        let (start, end) = result.unwrap();
        let extracted = &html[start..end];
        assert!(extracted.contains("Consolidated Financial Statements"));
        assert!(extracted.contains("Consolidated Statements of Operations"));
        assert!(!extracted.contains("Item 9. Changes in Accountants"));
    }
    
    #[test]
    fn test_toc_strategy() {
        setup_test_env();
        let html = r##"
        <!DOCTYPE html>
        <html>
        <body>
            <div class="table-of-contents">
                <a href="#item1">Item 1. Business</a>
                <a href="#item8">Item 8. Financial Statements and Supplementary Data</a>
                <a href="#item9">Item 9. Changes in Accountants</a>
            </div>
            
            <h1 id="item1">Item 1. Business</h1>
            <p>Business description goes here...</p>
            
            <h1 id="item8">Item 8. Financial Statements and Supplementary Data</h1>
            <p>Financial statements content...</p>
            <table>
                <tr><th>Assets</th><th>2023</th><th>2022</th></tr>
            </table>
            
            <h1 id="item9">Item 9. Changes in Accountants</h1>
            <p>Changes in accountants content...</p>
        </body>
        </html>
        "##;
        
        let strategy = TocExtractionStrategy;
        let result = strategy.extract(html);
        assert!(result.is_some());
        
        let (start, end) = result.unwrap();
        let extracted = &html[start..end];
        assert!(extracted.contains("Item 8. Financial Statements"));
        assert!(!extracted.contains("Item 9. Changes"));
    }
    
    #[test]
    fn test_tocguided_strategy() {
        setup_test_env();
        let html = create_test_html_with_toc(
            "Item 8. Financial Statements and Supplementary Data", 
            "Item 9. Changes in and Disagreements with Accountants"
        );
        
        let strategy = TocExtractionStrategy;
        let result = strategy.extract(&html);
        assert!(result.is_some());
        
        let (start, end) = result.unwrap();
        let extracted = &html[start..end];
        assert!(extracted.contains("Item 8. Financial Statements"));
        assert!(extracted.contains("Consolidated Balance Sheets"));
        assert!(!extracted.contains("Item 9. Changes"));
    }
    
    #[test]
    fn test_is_in_table_of_contents() {
        let html = create_test_html_with_toc(
            "Item 8. Financial Statements and Supplementary Data", 
            "Item 9. Changes in and Disagreements with Accountants"
        );
        
        // Find ToC position
        let toc_pos = html.find("Table of Contents").unwrap();
        assert!(is_in_table_of_contents(&html, toc_pos + 50)); // Should be in ToC
        
        // Find Item 8 position in the actual content
        let item8_pos = html.find("id=\"item8\"").unwrap();
        assert!(!is_in_table_of_contents(&html, item8_pos + 50)); // Should not be in ToC
    }
    
    #[test]
    fn test_section_extractor_integration() {
        setup_test_env();
        // Create a test HTML with multiple possible extraction patterns
        let html = r##"
        <!DOCTYPE html>
        <html>
        <body>
            <div class="table-of-contents">
                <a href="#item8">Item 8. Financial Statements</a>
            </div>
            
            <h1>PART II</h1>
            
            <h2 id="item8">Item 8. Financial Statements and Supplementary Data</h2>
            <p>This section contains financial information for the company.</p>
            
            <h3>Consolidated Statements of Operations</h3>
            <table>
                <tr><th>Revenue</th><th>2023</th><th>2022</th></tr>
                <tr><td>Total</td><td>5000</td><td>4500</td></tr>
            </table>
            
            <h2>Item 9. Changes in Accountants</h2>
        </body>
        </html>
        "##;
        
        let extractor = SectionExtractor::new();
        let result = extractor.extract_item_8(html, 2023, "Test Company", "TEST");
        
        assert!(result.is_ok());
        let section = result.unwrap();
        assert_eq!(section.section_name, "Item 8");
        assert_eq!(section.filing_year, 2023);
        assert_eq!(section.ticker, "TEST");
        assert!(section.content.contains("financial information"));
        assert!(section.content.contains("Consolidated Statements"));
        assert!(!section.content.contains("Item 9. Changes"));
    }

    #[test]
    fn test_multiple_strategy_fallback() {
        setup_test_env();
        // Create HTML without standard Item 8 heading but with financial statements
        let html = r#"
        <!DOCTYPE html>
        <html>
        <body>
            <h1>PART II</h1>
            <h2>Financial Information</h2>
            <p>The following financial statements are presented.</p>
            
            <h3>Consolidated Balance Sheets</h3>
            <table>
                <tr><th>Assets</th><th>2023</th><th>2022</th></tr>
            </table>
            
            <h3>Consolidated Statements of Income</h3>
            <table>
                <tr><th>Revenue</th><th>2023</th><th>2022</th></tr>
            </table>
            
            <h2>Item 9. Changes in Accountants</h2>
        </body>
        </html>
        "#;
        
        let extractor = SectionExtractor::new();
        let result = extractor.extract_item_8(html, 2023, "Test Company", "TEST");
        
        // The extractor should fall back to the financial statement strategy
        assert!(result.is_ok());
        let section = result.unwrap();
        assert!(section.content.contains("Consolidated Balance Sheets"));
        assert!(section.content.contains("Consolidated Statements of Income"));
    }
    
    #[test]
    fn test_actual_item8_strategy() {
        setup_test_env();
        let html = r##"
    <!DOCTYPE html>
    <html>
    <body>
        <h1>Table of Contents</h1>
        <a href="#item1">Item 1. Business</a><br>
        <a href="#item8">Item 8. Financial Statements</a><br>
        
        <h1>PART I</h1>
        <h2 id="item1">Item 1. Business</h2>
        
        <h1>PART II</h1>
        <h2 id="item8">Item 8. Financial Statements and Supplementary Data</h2>
        <p>Our financial statements begin on the following page.</p>
        
        <h3>Report of Independent Registered Public Accounting Firm</h3>
        <p>We have audited the consolidated financial statements...</p>
        
        <h3>Consolidated Balance Sheets</h3>
        <table>
            <tr><th>Assets</th><th>2023</th></tr>
            <tr><td>Cash</td><td>1000</td></tr>
        </table>
        
        <h2>Item 9. Changes in Accountants</h2>
    </body>
    </html>
    "##;
        
        let strategy = ActualItem8ExtractionStrategy;
        let result = strategy.extract(html);
        assert!(result.is_some());
        
        let (start, end) = result.unwrap();
        let extracted = &html[start..end];
        assert!(extracted.contains("Item 8. Financial Statements"));
        assert!(extracted.contains("Report of Independent"));
        assert!(extracted.contains("Consolidated Balance Sheets"));
        assert!(!extracted.contains("Item 9. Changes"));
    }
}