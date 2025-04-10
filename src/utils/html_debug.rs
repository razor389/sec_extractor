// src/utils/html_debug.rs
use std::fs::File;
use std::io::Write;
use std::path::Path;
use crate::utils::error::AppError;

/// Saves a HTML snippet to a file with debug highlights
pub fn save_debug_html(html: &str, filename: &str, highlights: &[(usize, usize, &str)]) -> Result<(), AppError> {
    let path = Path::new(filename);
    let mut file = File::create(path)?;
    
    // Add debug styling in head
    let mut debug_html = String::from("<!DOCTYPE html>\n<html>\n<head>\n<style>\n");
    
    // CSS for highlight colors
    debug_html.push_str(".highlight-start { background-color: #FFFF00; }\n");
    debug_html.push_str(".highlight-end { background-color: #FFA500; }\n");
    debug_html.push_str(".highlight-item8 { background-color: #90EE90; }\n");
    debug_html.push_str(".highlight-item9 { background-color: #ADD8E6; }\n");
    debug_html.push_str(".highlight-custom { background-color: #FFC0CB; }\n");
    debug_html.push_str("</style>\n</head>\n<body>\n");
    
    // Create the modified HTML with markers
    let mut last_pos = 0;
    let mut sorted_highlights = highlights.to_vec();
    sorted_highlights.sort_by_key(|h| h.0); // Sort by position
    
    for (start, end, highlight_type) in sorted_highlights {
        // Add content before the highlight
        if start > last_pos {
            debug_html.push_str(&html[last_pos..start]);
        }
        
        // Determine CSS class based on highlight type
        let css_class = match highlight_type {
            "start" => "highlight-start",
            "end" => "highlight-end",
            "item8" => "highlight-item8",
            "item9" => "highlight-item9",
            _ => "highlight-custom",
        };
        
        // Add the highlighted section with a marker
        debug_html.push_str(&format!("<span class=\"{}\" title=\"Position: {}-{}, Type: {}\">", 
            css_class, start, end, highlight_type));
        debug_html.push_str(&html[start..end]);
        debug_html.push_str("</span>");
        
        last_pos = end;
    }
    
    // Add any remaining content
    if last_pos < html.len() {
        debug_html.push_str(&html[last_pos..]);
    }
    
    // Close the HTML document
    debug_html.push_str("\n</body>\n</html>");
    
    // Write to file
    file.write_all(debug_html.as_bytes())?;
    
    tracing::info!("Saved debug HTML to {}", path.display());
    Ok(())
}

/// Creates a debug version of an HTML document with locations of specified regex patterns highlighted
pub fn create_debug_html(html: &str, filename: &str, patterns: &[(&str, &str)]) -> Result<(), AppError> {
    use regex::Regex;
    
    let mut highlights = Vec::new();
    
    // Find all matches for each pattern and add them to highlights
    for (pattern, highlight_type) in patterns {
        let re = Regex::new(pattern).map_err(|e| {
            AppError::Config(format!("Invalid regex pattern '{}': {}", pattern, e))
        })?;
        
        for mat in re.find_iter(html) {
            highlights.push((mat.start(), mat.end(), *highlight_type));
        }
    }
    
    save_debug_html(html, filename, &highlights)
}