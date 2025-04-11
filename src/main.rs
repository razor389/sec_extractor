// src/main.rs
mod utils;
mod edgar;
mod extractors;
mod storage;

use clap::Parser; // <<< Ensure this use statement is present
use utils::AppError;
use edgar::client;
use extractors::section::DomExtractor;
use storage::StorageManager;

/// Command Line Interface for SEC Item 8 Parser
#[derive(Parser, Debug)] // <<< derive(Parser) needs 'use clap::Parser;'
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

    /// Debug mode - save raw HTML files for debugging failures
    #[arg(short, long)]
    debug: bool,

    /// Set minimum section size in bytes (default: 1000)
    #[arg(long, default_value = "1000")]
    min_section_size: usize,
}

#[tokio::main]
async fn main() -> Result<(), AppError> {
    // 1. Setup Logging
    utils::logging::setup_logging();

    // 2. Parse CLI Arguments
    let args = Args::parse(); // <<< This uses clap::Parser
    tracing::info!("Starting processing for args: {:?}", args);
    tracing::debug!("Minimum section size set to: {}", args.min_section_size);

    // 3. Initialize storage
    let storage = StorageManager::new(&args.output_dir)?;

    // 4. Initialize section extractor
    let section_extractor = DomExtractor::new();

    // 5. If accession number is provided, process just that filing (Placeholder)
    if let Some(accession) = &args.accession_number {
        tracing::info!("Processing specific filing: {}", accession);
        // TODO: Implement specific filing processing logic here
        // You would need to fetch company info differently or assume it's known
        // Then download the specific filing using a constructed URL
        return Err(AppError::Config("Processing by accession number not yet implemented".to_string()));
    }

    // --- Added Missing Logic: Fetching Filings and Loop ---
    // 6. Find 10-K filings for the ticker
    tracing::info!("Finding 10-K filings for ticker: {}", args.ticker);
    let filings = client::find_10k_filings(
        &args.ticker,
        args.start_year,
        args.end_year
    ).await?; // <<< Define 'filings'

    tracing::info!("Found {} 10-K filings", filings.len());

    if filings.is_empty() {
        return Err(AppError::Config(format!("No 10-K filings found for ticker {} in the specified date range", args.ticker)));
    }

    // 7. Process each filing - Initialize counters outside the loop
    let mut success_count = 0; // <<< Define counters
    let mut failure_count = 0;

    for filing in filings { // <<< Start the loop, defines 'filing'
        tracing::info!("Processing filing for year: {:?} ({})", filing.year, filing.accession_number);

        // Download the filing document
        let url = filing.primary_doc_url(); // <<< Define 'url'
        tracing::info!("Downloading from URL: {}", url);

        match client::download_filing_doc(&url).await {
            Ok(content) => {
                tracing::info!("Successfully downloaded document ({} bytes)", content.len());

                // --- Debugging Section ---
                // Create debug directory path regardless of args.debug, used for failure logs too
                 let debug_dir = format!("{}/{}/{}/debug",
                                        args.output_dir,
                                        filing.ticker.to_uppercase(),
                                        filing.year.unwrap_or(0)); // Use 0 if year is None for path


                if args.debug {
                    // Ensure debug directory exists
                     if let Err(e) = std::fs::create_dir_all(&debug_dir) {
                         tracing::error!("Failed to create debug directory {}: {}", debug_dir, e);
                         // Decide if this is fatal or just skip debug saving
                     } else {
                         // Save the raw filing for debugging
                         let raw_filing_path = format!("{}/raw_filing.html", debug_dir);
                         if let Err(e) = std::fs::write(&raw_filing_path, &content) {
                             tracing::error!("Failed to save raw filing to {}: {}", raw_filing_path, e);
                         } else {
                             tracing::info!("Saved raw filing to: {}", raw_filing_path);
                         }
                     }

                    // TODO: Update or remove html_debug::create_debug_html
                    tracing::warn!("Skipping annotated debug HTML generation - needs rework for DOM approach.");
                }

                // --- Extraction Call ---
                if let Some(year) = filing.year { // <<< Check optional year
                     match section_extractor.extract_item_8(
                        &content,
                        year,
                        &filing.company_name,
                        &filing.ticker,
                        args.min_section_size
                    ) {
                        Ok(section) => {
                            tracing::info!("Successfully extracted Item 8 section ({} bytes)", section.content_html.len());
                            success_count += 1; // <<< Increment counter

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
                            tracing::error!("Failed to extract Item 8 section for {}-{}: {}", filing.ticker, year, e);
                            failure_count += 1; // <<< Increment counter
                            // Optional: Save failure info even if not in debug mode, but save to debug dir
                            // Ensure debug directory exists before writing failure info
                             if let Err(e) = std::fs::create_dir_all(&debug_dir) {
                                 tracing::error!("Failed to create debug directory {} for failure log: {}", debug_dir, e);
                             } else {
                                 let failure_info_path = format!("{}/extraction_failure.txt", debug_dir);
                                 let failure_info = format!("Failed to extract Item 8 for {} {}: {}\nURL: {}\n",
                                                            filing.ticker, year, e, url); // <<< Use defined 'url'
                                 if let Err(write_err) = std::fs::write(&failure_info_path, failure_info) {
                                     tracing::error!("Failed to save failure info: {}", write_err);
                                 } else {
                                     tracing::debug!("Saved extraction failure info to {}", failure_info_path);
                                 }
                             }
                        }
                    }
                } else {
                    tracing::warn!("Filing year not available for {}, skipping extraction", filing.accession_number);
                    failure_count += 1; // <<< Increment counter
                }
            },
            Err(e) => {
                tracing::error!("Failed to download filing document {}: {}", url, e); // <<< Use defined 'url'
                failure_count += 1; // <<< Increment counter
            }
        }
    } // <<< End of the loop

    // --- Final Summary ---
    tracing::info!("Processing finished. Success: {}, Failures: {}", success_count, failure_count);

    if success_count == 0 && failure_count > 0 {
        // Use a more specific error or just log and exit cleanly?
        // Returning an error might be better for scripting.
        // Consider not erroring out if *some* filings were processed, even if others failed.
        // For now, error out if ALL attempts failed.
         tracing::error!("All extraction attempts failed.");
         return Err(AppError::Processing(format!("Failed to extract any Item 8 sections from {} filings attempted", failure_count)));

    }

    Ok(())
}