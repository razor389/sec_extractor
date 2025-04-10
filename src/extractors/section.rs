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
                if let Some(mat) = re.find(html_content) {
                    start_pos = Some(mat.start());
                    tracing::debug!("Found start pattern match: '{}' at position: {}", pattern, mat.start());
                    break;
                }
            }
        }
        
        let start_pos = start_pos?;
        
        // Find end position
        let mut end_pos = None;
        for pattern in &self.end_patterns {
            if let Ok(re) = Regex::new(pattern) {
                if let Some(mat) = re.find(&html_content[start_pos..]) {
                    end_pos = Some(start_pos + mat.start());
                    tracing::debug!("Found end pattern match: '{}' at position: {}", pattern, start_pos + mat.start());
                    break;
                }
            }
        }
        
        // If no end marker found, return the rest of the document
        let end_pos = end_pos.unwrap_or_else(|| html_content.len());
        
        // Only return a result if the section is reasonably sized
        let min_section_size = get_min_section_size();
        if end_pos > start_pos && end_pos - start_pos > min_section_size {
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
        let item8_mat = item8_re.find(&html_content[part2_pos..])?;
        let start_pos = part2_pos + item8_mat.start();
        
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
        
        Some((start_pos, end_pos))
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
}