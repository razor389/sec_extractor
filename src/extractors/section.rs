// src/extractors/section.rs

// --- Imports ---
use crate::utils::error::ExtractError;
use regex::Regex;
use scraper::{Html, Selector, ElementRef, node::Node}; // Import Node enum if needed for pattern matching
use once_cell::sync::Lazy;

// --- Constants ---
// We might still need some constants, but many old ones related to byte offsets are gone.
// const START_VALIDATION_LOOKAHEAD: usize = 5000; // May adapt this concept later if needed
const FALLBACK_END_CHUNK_SIZE: usize = 350_000; // Might still need a fallback size limit

// --- CSS Selectors (Lazy Static) ---
// Selectors for potential section headers
static POTENTIAL_HEADER_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    // Added 'div', 'span', and 'a' to catch more heading/link structures
    Selector::parse("h1, h2, h3, h4, h5, h6, p > b, p > strong, div > b, div > strong, font, div, span, a")
        .expect("Failed to compile POTENTIAL_HEADER_SELECTOR")
});

// Selectors for potential ToC containers (can be refined)
static TOC_CONTAINER_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("div[class*='toc'], nav[class*='toc'], div[id*='toc'], nav[id*='toc']") // Check class/id containing 'toc'
        .expect("Failed to compile TOC_CONTAINER_SELECTOR")
});

// Selectors for common block elements that might signal end of ToC visually
static TOC_END_SIBLING_SELECTOR: Lazy<Selector> = Lazy::new(|| {
    Selector::parse("h1, h2, h3, table, hr, div[style*='page-break']") // Elements likely following a ToC
        .expect("Failed to compile TOC_END_SIBLING_SELECTOR")
});


// --- Regex Patterns for Text Matching (Lazy Static) ---
// Adapt existing patterns for text matching *within* selected elements
static ITEM_8_START_TEXT_RE: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        // Pattern 1: Match "Item 8." plus the full title, anchored to start/end of cleaned text
        // Allows optional period after 8, optional period/space at the very end.
        r"(?i)^\s*Item\s*8\.?\s*Financial\s*Statements\s*(?:and\s*Supplementary\s*Data)?\.?\s*$",

        // Pattern 2: Match "Item 8." followed by "Financial Statements" anywhere after word boundary
        // This catches cases where extra text might follow the main title within the same element
        r"(?i)\bItem\s*8[\.\s\-–—:]+Financial\s*Statements",

        // Pattern 3: REMOVED / COMMENTED OUT - Too broad, caused false positive
        // r"(?i)\bItem\s*8\.?",
    ]
    .iter()
    .filter_map(|pat| Regex::new(pat).ok()) // Use filter_map for cleaner error handling on regex creation
    .collect()
});

// Patterns to identify the *end* of Item 8 (start of Item 9, Part III, etc.)
static ITEM_8_END_TEXT_RE: Lazy<Vec<Regex>> = Lazy::new(|| {
     [
        // Item 9 is the most common and reliable end marker
        r"(?i)^Item\s*9[ABC]?\.?\s*[Cc]hanges\b", // Starts with Item 9(A/B/C). Changes
        r"(?i)\bItem\s*9[ABC]?\.?\s", // Contains Item 9(A/B/C).

        // Part III is another strong indicator
        r"(?i)^PART\s+III\b", // Starts with PART III
        r"(?i)\bPART\s+III\b", // Contains PART III

        // Item 10 is less common immediately after 8 but possible
        r"(?i)^Item\s*10\.?\s*[Dd]irectors\b",
        r"(?i)\bItem\s*10\.?\s",

        // Signatures / Exhibits are usually much later but act as fallbacks
        r"(?i)\bSIGNATURES\b",
        r"(?i)\bEXHIBIT\s+INDEX\b",
        r"(?i)\bEXHIBITS?\b",
     ]
    .iter()
    .filter_map(|pat| Regex::new(pat).ok())
    .collect()
});

// --- Data Structures ---
#[derive(Debug, Clone)]
pub struct ExtractedSection {
    pub section_name: String,  // e.g., "Item 8"
    pub section_title: String, // e.g., "Financial Statements and Supplementary Data" (best effort)
    pub content_html: String,  // The raw HTML content of the section
    pub filing_year: u32,      // The year of the filing
    pub company_name: String,  // Company name
    pub ticker: String,        // Ticker symbol
    // Add fields for XBRL later if needed
    // pub xbrl_facts: Vec<XbrlFact>,
}

// --- Main Extractor Structure (Refactored) ---
pub struct DomExtractor; // Renamed for clarity

impl DomExtractor {
    pub fn new() -> Self { Self {} }

    /// Extracts Item 8 content using DOM traversal and text matching.
    pub fn extract_item_8(
        &self,
        html_content: &str,
        filing_year: u32,
        company_name: &str,
        ticker: &str,
        min_section_size: usize,
    ) -> Result<ExtractedSection, ExtractError> {
        tracing::info!("Attempting DOM-based extraction for Item 8: {} ({}), min size {}", ticker, filing_year, min_section_size);

        // 1. Parse the HTML document
        let document = Html::parse_document(html_content);

        // 2. Find the start and end element boundaries for Item 8
        let (start_element, end_element) = self.find_section_boundaries(&document, "Item 8", &ITEM_8_START_TEXT_RE, &ITEM_8_END_TEXT_RE)
            .ok_or_else(|| ExtractError::SectionNotFound(format!("Could not find valid start/end boundaries for Item 8 in DOM for {}-{}", ticker, filing_year)))?;

        tracing::debug!("Found potential Item 8 start element: {:?}", start_element.value().name());
        tracing::debug!("Found potential Item 8 end marker element: {:?}", end_element.value().name());

        // 3. Extract the HTML content between the identified elements
        let section_html = self.extract_html_between(start_element, end_element)?;
        let section_size = section_html.len();

        // 4. Basic Validation (Size Check)
        if section_size < min_section_size {
            tracing::error!("Extracted Item 8 DOM section is too small ({} bytes, required {}) for ticker {} ({}).", section_size, min_section_size, ticker, filing_year);
            return Err(ExtractError::SectionNotFound(format!("Item 8 found but size {} bytes is less than minimum {} bytes", section_size, min_section_size)));
        }

        // 5. (Optional but Recommended) Final Content Validation
        //    Could check `section_html` for keywords or presence of XBRL tags if needed.
        //    Example: if !self.validate_financial_content_dom(&section_html) { ... return Err ... }


        tracing::info!("Successfully extracted Item 8 via DOM for {} ({}): {} bytes", ticker, filing_year, section_size);
        Ok(ExtractedSection {
            section_name: "Item 8".to_string(),
            // TODO: Try to extract a better title from the start_element text
            section_title: "Financial Statements and Supplementary Data".to_string(),
            content_html: section_html,
            filing_year,
            company_name: company_name.to_string(),
            ticker: ticker.to_string(),
        })
    }

    /// Finds the start and end ElementRefs for a named section.
    /// Searches for potential headers, validates text, checks ToC, finds end marker.
    fn find_section_boundaries<'a>(
        &self,
        document: &'a Html,
        section_name: &str, // e.g., "Item 8"
        start_patterns: &[Regex],
        end_patterns: &[Regex],
    ) -> Option<(ElementRef<'a>, ElementRef<'a>)> {

        let mut best_start_element: Option<ElementRef> = None;

        // Iterate through potential header elements defined by the selector
        for element in document.select(&POTENTIAL_HEADER_SELECTOR) {
            let element_text = element.text().collect::<String>();
            let cleaned_text = element_text
                .trim()
                .replace("\n", " ")
                .replace("&nbsp;", " ")
                .replace("&#160;", " ");

            // Check if element text matches any start patterns
            if start_patterns.iter().any(|re| re.is_match(&cleaned_text)) {
                tracing::trace!("Found potential '{}' start element: '{}' (text: '{}')", section_name, element.value().name(), cleaned_text);

                // ** Crucial Check: Is this element likely part of the Table of Contents? **
                if self.is_in_toc_dom(element) {
                    tracing::debug!("Skipping potential start element - likely in ToC: '{}'", cleaned_text);
                    continue; // Skip this element, it's probably in the ToC
                }

                // ** Basic Content Lookahead (Optional but helpful) **
                // Simple check: does the *immediate* next content look promising?
                // (e.g., a table, or text containing keywords) - This is a simpler validation than the old byte-based one.
                // if !self.peek_ahead_for_content(element) {
                //     tracing::debug!("Skipping potential start - lookahead check failed for: '{}'", cleaned_text);
                //     continue;
                // }

                // If we passed the ToC check (and optionally lookahead), this is our candidate start
                // We take the *first* valid one found based on document order.
                best_start_element = Some(element);
                tracing::info!("Selected candidate start element for {}: {:?} '{}'", section_name, element.value().name(), cleaned_text);
                break; // Stop searching for start markers
            }
        }

        // If no valid start element found, return None
        let start_element = best_start_element?;
        tracing::debug!("Confirmed start element for {}: {:?}", section_name, start_element.id());


        // --- Find the End Marker ---
        // Search *after* the start element for the *first* element matching end patterns.
        let mut potential_end_element: Option<ElementRef> = None;
        for element in start_element.next_siblings().flat_map(|node| ElementRef::wrap(node)) {
             // Recursively check descendants as well? Maybe too complex for now.
             // Let's first check the direct siblings and their header-like children.
            for descendant in element.select(&POTENTIAL_HEADER_SELECTOR) { // Check headers within siblings
                 let descendant_text = descendant.text().collect::<String>();
                 let cleaned_text = descendant_text.trim().replace("\n", " ").replace("&nbsp;", " ");

                 if end_patterns.iter().any(|re| re.is_match(&cleaned_text)) {
                     // Found a potential end marker
                     tracing::debug!("Found potential end marker for '{}' after start: {:?} '{}'", section_name, descendant.value().name(), cleaned_text);
                     potential_end_element = Some(descendant);
                     break; // Found the first end marker, stop searching this branch
                 }
            }
             if potential_end_element.is_some() { break; } // Stop searching siblings if end found

             // Also check the top-level sibling itself if it's a header
             if let Some(name) = element.value().name().to_lowercase().split('.').next() {
                 if ["h1","h2","h3","h4","h5","h6","p","div","font"].contains(&name) { // Check common structural/header tags
                     let element_text = element.text().collect::<String>();
                     let cleaned_text = element_text.trim().replace("\n", " ").replace("&nbsp;", " ");
                      if end_patterns.iter().any(|re| re.is_match(&cleaned_text)) {
                         tracing::debug!("Found potential end marker (sibling) for '{}' after start: {:?} '{}'", section_name, element.value().name(), cleaned_text);
                         potential_end_element = Some(element);
                         break; // Found the first end marker, stop searching siblings
                     }
                 }
             }
              if potential_end_element.is_some() { break; } // Stop searching siblings if end found
        }


        // TODO: Handle case where no end marker is found more gracefully
        // Maybe search until end of document or use a fallback size limit?
        let end_element = potential_end_element.or_else(|| {
             tracing::warn!("No specific end marker found for '{}' after start element. Finding end of document may be needed.", section_name);
             // Placeholder: Need a better way to find the "end" if no marker exists
             // For now, maybe just return None which causes the main function to error out.
             None
         })?;


        Some((start_element, end_element))
    }


    /// Checks if an element is likely within a Table of Contents using DOM structure.
    fn is_in_toc_dom(&self, element: ElementRef) -> bool {
        tracing::trace!("Checking ToC for element <{}> | Text: '{}'", element.value().name(), element.text().collect::<String>().trim()); // Added text to log
    
        // Check 1: Is the element itself an anchor tag with an href? (Strong ToC indicator)
        if element.value().name() == "a" && element.value().attr("href").is_some() {
             tracing::debug!("Element itself is <a> tag with href, likely ToC link.");
             return true;
        }
    
        // Check 2: Traverse ancestors looking for clues
        let mut table_ancestor_found = false; // Flag to check context
        for ancestor_node in element.ancestors() {
            if let Some(ancestor) = ElementRef::wrap(ancestor_node) {
                let ancestor_name = ancestor.value().name();
                tracing::trace!(" Checking ancestor <{}>", ancestor_name);
    
                // Check standard ToC container selector (class/id contains 'toc')
                if TOC_CONTAINER_SELECTOR.matches(&ancestor) {
                    tracing::debug!(" Element has ancestor matching TOC_CONTAINER_SELECTOR ({}), confirmed ToC.", ancestor_name);
                    return true;
                }
    
                // Check if an ancestor is an anchor tag (element is *inside* a link)
                if ancestor_name == "a" && ancestor.value().attr("href").is_some() {
                     tracing::debug!("Element has an ancestor <a> tag with href, likely ToC link structure.");
                     return true;
                }
    
                // Check for table structure - set flag but don't return immediately
                if ["td", "tr", "table"].contains(&ancestor_name) {
                     table_ancestor_found = true;
                     tracing::trace!(" Found table ancestor: {}", ancestor_name);
                }
    
    
                if ancestor_name == "body" {
                    tracing::trace!(" Reached body, stopping ancestor check.");
                    break;
                }
            }
        }
    
        // Check 3: Contextual check - Element looks like a heading but is inside a table structure?
        // This is less certain, but can help for ToCs not marked with class/id="toc"
        // Only apply if it wasn't already confirmed by checks 1 or 2.
        if table_ancestor_found {
            // If it's inside a table structure AND looks like a simple "Item X." link text, it's likely ToC
            let element_text = element.text().collect::<String>();
            let cleaned_text = element_text.trim().replace("&nbsp;", " ").replace("&#160;", " "); // Basic clean
             // Example heuristic: Check if it looks like just "Item <number>." - common in ToC links
            let simple_item_regex = Regex::new(r"^\s*Item\s+\d+[A-Z]?\.?\s*$").unwrap();
            if simple_item_regex.is_match(&cleaned_text) {
                 tracing::debug!("Element has table ancestor AND matches simple 'Item X.' pattern, likely ToC.");
                 return true;
            }
            tracing::trace!("Element has table ancestor, but text doesn't match simple ToC pattern.");
        }
    
    
        tracing::trace!("Element not definitively identified within a known ToC structure.");
        false // Default: Assume not in ToC if no checks match
    }

    /// Extracts the raw HTML string for all nodes between start_el (exclusive) and end_el (exclusive).
    fn extract_html_between<'a>(
        &self,
        start_el: ElementRef<'a>,
        end_el: ElementRef<'a>,
    ) -> Result<String, ExtractError> {
        let mut content = String::new();

        // Iterate over nodes that are siblings *after* the start_el node.
        // start_el.next_siblings() returns an iterator over ego_tree::NodeRef<'a, Node>.
        for node in start_el.next_siblings() { // <<< Use the next_siblings() iterator directly

            // Check if the current node's ID is the same as the end element's ID
            if node.id() == end_el.id() { // <<< Compare node IDs directly
                break; // Stop when we reach the end element's node
            }

            // Append the HTML representation of the node
            // ElementRef::wrap takes a NodeRef, which 'node' already is.
            if let Some(el_ref) = ElementRef::wrap(node) {
                content.push_str(&el_ref.html());
            } else {
                // Handle other node types, primarily Text nodes
                match node.value() {
                    Node::Text(text_node) => {
                        // Escape if needed for rendering, direct text for raw extraction
                        content.push_str(&text_node.text);
                    }
                    _ => {} // Ignore comments, etc.
                }
            }
        }

        Ok(content)
    }

     // Placeholder for content validation if needed
     // fn validate_financial_content_dom(&self, html_fragment: &str) -> bool { ... }

     // Placeholder for optional lookahead check
     // fn peek_ahead_for_content(&self, start_element: ElementRef) -> bool { ... }

}

// --- Old Helper Functions (To Be Removed or Replaced) ---
// fn is_in_table_of_contents(...) -> bool { ... REMOVED ... }
// fn contains_financial_content(...) -> bool { ... REMOVED or ADAPT ... }
// fn find_item_8_bounds(...) -> Option<(usize, usize)> { ... REMOVED ... }

// --- Tests ---
#[cfg(test)]
mod tests {
     use super::*;
     // Use a smaller min size for tests unless specifically testing size limits
     const TEST_MIN_SIZE: usize = 50;

     #[test]
     fn test_placeholder_dom_extraction() {
         // Create test HTML suitable for DOM parsing
         let html = r#"
             <!DOCTYPE html>
             <html><head><title>Test</title></head><body>
             <h1>Some Doc</h1>
             <div class="toc"> <p><b>Item 8. Financial Statements</b>... Page 5</p> </div>
             <hr/>
             <h2>PART II</h2>
             <p>Some intro text for Part II.</p>
             <h2><b>Item 8. Financial Statements and Supplementary Data</b></h2>
             <p>This is the real start.</p>
             <table><tr><td>Assets</td><td>100</td></tr></table>
             <p>Some notes here.</p>
             <h3>Item 9. Changes in Accountants</h3>
             <p>End of Item 8 content.</p>
             </body></html>
         "#;

         let extractor = DomExtractor::new();
         let result = extractor.extract_item_8(html, 2023, "TestCo", "TST", TEST_MIN_SIZE);

         assert!(result.is_ok(), "DOM extraction failed: {:?}", result.err());

         if let Ok(section) = result {
             println!("Extracted HTML:\n{}", section.content_html);
             assert!(section.content_html.contains("This is the real start."), "Missing start content");
             assert!(section.content_html.contains("<table>"), "Missing table content");
             assert!(section.content_html.contains("Some notes here."), "Missing notes content");
             assert!(!section.content_html.contains("Item 8. Financial Statements and Supplementary Data"), "Should not contain the Item 8 header itself");
             assert!(!section.content_html.contains("Item 9."), "Should not contain the Item 9 header");
             assert!(!section.content_html.contains("<div class=\"toc\">"), "Should not contain ToC");
             assert!(section.content_html.len() > TEST_MIN_SIZE);
         }
     }

     #[test]
     fn test_toc_detection_dom() {
         let html_toc = r#"<body><div id="toc"><p><b>Item 8. Financials</b></p></div><hr/><h2>Item 8 Actual</h2></body>"#;
         let html_no_toc = r#"<body><hr/><h2>Item 8 Actual</h2></body>"#;
         let doc_toc = Html::parse_document(html_toc);
         let doc_no_toc = Html::parse_document(html_no_toc);

         let toc_header_selector = Selector::parse("p > b").unwrap();
         let actual_header_selector = Selector::parse("h2").unwrap();

         let toc_element = doc_toc.select(&toc_header_selector).next().unwrap();
         let actual_element_in_toc_doc = doc_toc.select(&actual_header_selector).next().unwrap();
         let actual_element_in_no_toc_doc = doc_no_toc.select(&actual_header_selector).next().unwrap();

         let extractor = DomExtractor::new();

         assert!(extractor.is_in_toc_dom(toc_element), "Should detect element within div#toc");
         assert!(!extractor.is_in_toc_dom(actual_element_in_toc_doc), "Should NOT detect element after ToC div");
         assert!(!extractor.is_in_toc_dom(actual_element_in_no_toc_doc), "Should NOT detect element when no ToC exists");
     }
}