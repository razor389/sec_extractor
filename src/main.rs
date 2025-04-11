// src/main.rs
mod utils;
mod edgar;
mod extractors;
mod storage;

use clap::Parser;
use utils::AppError;
use edgar::client;
use extractors::section::SectionExtractor;
use storage::StorageManager;

/// Command Line Interface for SEC Item 8 Parser
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Ticker symbol of the company
    #[arg(short, long)]
    ticker: String,

    /// Start year for the 10-K filings (optional)
    #[arg(long)]
    start_year: Option<u32>,

    /// End year for the 10-K filings (optional)
    #[arg(long)]
    end_year: Option<u32>,

    /// Specific SEC accession number (optional, overrides ticker/year)
    #[arg(short, long)]
    accession_number: Option<String>,
    
    /// Output directory for extracted content
    #[arg(short, long, default_value = "./output")]
    output_dir: String,
    
    /// Debug mode - save annotated HTML files for debugging
    #[arg(short, long)]
    debug: bool,
    
    /// Set minimum section size in bytes (default: 1000)
    #[arg(long, default_value = "1000")]
    min_section_size: Option<usize>,
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    // 1. Setup Logging (reads RUST_LOG env var)
    utils::logging::setup_logging();

    // 2. Parse CLI Arguments
    let args = Args::parse();
    tracing::info!("Starting processing for args: {:?}", args);
    
    // Set MIN_SECTION_SIZE environment variable from command-line args or default
    if let Some(size) = args.min_section_size {
        std::env::set_var("MIN_SECTION_SIZE", size.to_string());
        tracing::debug!("Setting MIN_SECTION_SIZE to {} from command-line argument", size);
    } else if std::env::var("MIN_SECTION_SIZE").is_err() {
        std::env::set_var("MIN_SECTION_SIZE", "1000");
        tracing::debug!("Setting MIN_SECTION_SIZE to 1000 (default)");
    } else {
        let size = std::env::var("MIN_SECTION_SIZE").unwrap_or_else(|_| "1000".to_string());
        tracing::debug!("Using existing MIN_SECTION_SIZE: {}", size);
    }
    
    // 3. Initialize storage
    let storage = StorageManager::new(&args.output_dir)?;
    
    // 4. Initialize section extractor
    let section_extractor = SectionExtractor::new();
    
    // 5. If accession number is provided, process just that filing
    if let Some(accession) = &args.accession_number {
        tracing::info!("Processing specific filing: {}", accession);
        // TODO: Implement specific filing processing
        return Err(AppError::Config("Processing by accession number not yet implemented".to_string()));
    }
    
    // 6. Find 10-K filings for the ticker
    tracing::info!("Finding 10-K filings for ticker: {}", args.ticker);
    let filings = client::find_10k_filings(
        &args.ticker, 
        args.start_year, 
        args.end_year
    ).await?;
    
    tracing::info!("Found {} 10-K filings", filings.len());
    
    if filings.is_empty() {
        return Err(AppError::Config(format!("No 10-K filings found for ticker {} in the specified date range", args.ticker)));
    }
    
    // 7. Process each filing
    let mut success_count = 0;
    let mut failure_count = 0;
    
    for filing in filings {
        tracing::info!("Processing filing for year: {:?} ({})", filing.year, filing.accession_number);
        
        // Download the filing document
        let url = filing.primary_doc_url();
        tracing::info!("Downloading from URL: {}", url);
        
        match client::download_filing_doc(&url).await {
            Ok(content) => {
                tracing::info!("Successfully downloaded document ({} bytes)", content.len());
                
                // Extract Item 8
                if let Some(year) = filing.year {
                    // Create debug directory if needed
                    if args.debug {
                        let debug_dir = format!("{}/{}/{}/debug", 
                            args.output_dir, 
                            filing.ticker.to_uppercase(), 
                            year);
                        std::fs::create_dir_all(&debug_dir)?;
                        
                        // Save the raw filing for debugging
                        let raw_filing_path = format!("{}/raw_filing.html", debug_dir);
                        std::fs::write(&raw_filing_path, &content)?;
                        tracing::info!("Saved raw filing to: {}", raw_filing_path);
                        
                        // Create debug HTML with highlighted patterns
                        // Create debug HTML with highlighted patterns
                        let debug_patterns = [
                            (r"(?i)<h[1-6][^>]*>\s*Item\s*8\.?\s*Financial\s*Statements\s*and\s*Supplementary\s*Data\s*</h[1-6]>", "item8"),
                            (r"(?i)item\s*8[\.\s]*\(?financial\s+statements\s+and\s+supplementary\s+data\)?", "item8"),
                            (r"(?i)<h[1-6][^>]*>\s*consolidated\s+statements\s+of\s+operations\s*</h[1-6]>", "item8"),
                            (r"(?i)<h[1-6][^>]*>\s*consolidated\s+financial\s+statements\s*</h[1-6]>", "item8"),
                            (r"(?i)<h[1-6][^>]*>\s*notes\s+to\s+consolidated\s+financial\s+statements\s*</h[1-6]>", "item8"),
                            (r"(?i)<h[1-6][^>]*>\s*Item\s*9\.?\s*Changes\s*in\s*and\s*Disagreements\s*with\s*Accountants\s*</h[1-6]>", "item9"),
                            (r"(?i)item\s*9[\.\s]*\(?changes", "item9"),
                            (r"(?i)<h[1-6][^>]*>\s*PART\s*II\s*</h[1-6]>", "start"),
                            (r"(?i)<h[1-6][^>]*>\s*PART\s*III\s*</h[1-6]>", "end"),
                            (r"(?i)table\s+of\s+contents", "toc"),
                            // FIXED: Use alternate raw string delimiters to allow unescaped quotes.
                            (r#"(?i)<div[^>]*class=['"]?(?:toc|tableOfContents|index)['"]?[^>]*>"#, "toc"),
                            (r#"(?i)<a[^>]*href="[^"]*(?:item[_\-]?8|financial[_\-]statements)[^"]*"[^>]*>.*?item\s*8.*?</a>"#, "toclink"),
                        ];
                        let debug_html_path = format!("{}/filing_annotated.html", debug_dir);
                        if let Err(e) = utils::html_debug::create_debug_html(&content, &debug_html_path, &debug_patterns) {
                            tracing::warn!("Failed to create debug HTML: {}", e);
                        } else {
                            tracing::info!("Created annotated debug HTML: {}", debug_html_path);
                        }
                    }
                    
                    // Try to extract Item 8 using our extractor
                    match section_extractor.extract_item_8(&content, year, &filing.company_name, &filing.ticker) {
                        Ok(section) => {
                            tracing::info!("Successfully extracted Item 8 section ({} bytes)", section.content.len());
                            success_count += 1;
                            
                            // Save the section content
                            match storage.save_section(&section) {
                                Ok(path) => tracing::info!("Saved section content to: {}", path.display()),
                                Err(e) => tracing::error!("Failed to save section content: {}", e),
                            }
                            
                            // Save the section metadata
                            match storage.save_section_metadata(&section) {
                                Ok(path) => tracing::info!("Saved section metadata to: {}", path.display()),
                                Err(e) => tracing::error!("Failed to save section metadata: {}", e),
                            }
                        },
                        Err(e) => {
                            tracing::error!("Failed to extract Item 8 section: {}", e);
                            failure_count += 1;
                            
                            if args.debug {
                                // Save failure information for debugging
                                let debug_dir = format!("{}/{}/{}/debug", 
                                    args.output_dir, 
                                    filing.ticker.to_uppercase(), 
                                    year);
                                let failure_info_path = format!("{}/extraction_failure.txt", debug_dir);
                                let failure_info = format!("Failed to extract Item 8 for {} {}: {}\n", 
                                    filing.ticker, year, e);
                                if let Err(e) = std::fs::write(&failure_info_path, failure_info) {
                                    tracing::error!("Failed to save failure info: {}", e);
                                }
                            }
                        }
                    }
                } else {
                    tracing::warn!("Filing year not available, skipping extraction");
                }
            },
            Err(e) => {
                tracing::error!("Failed to download filing document: {}", e);
                failure_count += 1;
            }
        }
    }

    tracing::info!("Processing finished. Success: {}, Failures: {}", success_count, failure_count);
    
    if success_count == 0 && failure_count > 0 {
        return Err(AppError::Processing(format!("Failed to extract any Item 8 sections from {} filings", failure_count)));
    }
    
    Ok(())
}