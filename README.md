# SEC Filing Item 8 Extractor (Rust)

## Overview

This project aims to automate the extraction and parsing of "Item 8. Financial Statements and Supplementary Data" from SEC 10-K filings for publicly traded companies.

Recognizing the challenges of inconsistent formatting and potentially malformed HTML in EDGAR filings, this project now employs a **DOM-centric parsing strategy**. Instead of relying purely on regular expressions over the raw HTML string, it uses the `scraper` library to parse the filing into a Document Object Model (DOM). This allows for more robust navigation and section identification based on HTML structure, combined with text pattern matching within specific elements.

The primary goal remains to fetch relevant filings, accurately identify and isolate the Item 8 section, and prepare its content for further analysis (potentially including future table extraction, XBRL parsing, and LLM processing).

## Key Features (Current & Planned)

* **EDGAR Interaction:** Fetches company filing indexes and specific filing documents via the SEC EDGAR API, adhering to rate limits and user-agent requirements (`reqwest`).
* **DOM-Based Item 8 Extraction:** Parses the filing HTML into a DOM using `scraper`. Locates potential Item 8 boundaries by finding relevant HTML elements (headings, paragraphs) using CSS selectors, validating their text content with `regex`, and performing DOM-based checks to exclude Table of Contents entries.
* **Content Extraction:** Extracts the HTML content between the identified start and end DOM elements for the target section.
* **CLI Interface:** Provides a command-line interface using `clap` for specifying tickers, years, and other options.
* **Persistence:** Saves extracted sections and basic metadata to the local filesystem (`storage` module).
* **(Planned) XBRL Parsing:** Future integration of XML parsing (`roxmltree`) to extract structured financial data from embedded iXBRL tags within the identified section.
* **(Planned) Table Extraction:** Future implementation of logic to identify and parse HTML tables within Item 8.
* **(Planned) AI Integration:** Long-term goal to chunk extracted content for analysis by Large Language Models (LLMs).

## Parsing Strategy (DOM-Centric)

1. **Parse HTML to DOM:** The raw HTML content of the 10-K filing is parsed using `scraper::Html::parse_document`, creating an in-memory DOM tree that tolerates common HTML errors.
2. **Locate Candidate Elements:** CSS selectors (`scraper::Selector`) are used to identify potential elements that mark the *start* (e.g., `h2`, `p > b`) and *end* (e.g., `h2` for Item 9, `h1` for Part III) of the target section (Item 8).
3. **Validate Start Element:**
    * The text content of candidate start elements is checked against regular expressions (`regex`) for patterns like "Item 8".
    * A **DOM-based Table of Contents (ToC) check** (`is_in_toc_dom`) traverses the element's ancestors to see if it resides within a likely ToC container (e.g., `<div class="toc">`). Candidates within the ToC are skipped.
4. **Identify End Element:** Once a valid start element is confirmed, the parser searches subsequent sibling nodes in the DOM for the first element matching the defined end patterns (e.g., "Item 9", "PART III").
5. **Extract HTML Content:** The HTML content of all nodes *between* the validated start element and the identified end element is extracted (`extract_html_between`).
6. **(Future) Extract XBRL:** Logic will be added to find iXBRL tags within the extracted section's scope and parse them using an XML library like `roxmltree`.

## Project Structure

```text
.
├── .gitignore
├── Cargo.toml
├── README.md
└── src/
    ├── edgar/
    │   ├── client.rs      # SEC EDGAR API interaction (reqwest)
    │   ├── mod.rs
    │   └── models.rs      # EDGAR data models (serde)
    ├── extractors/
    │   ├── mod.rs
    │   └── section.rs     # DOM-based section extraction (scraper, regex)
    ├── main.rs          # Entry point and CLI handling (clap, tokio)
    ├── storage/
    │   └── mod.rs         # Saving extracted data to disk
    └── utils/
        ├── error.rs       # Custom error handling (thiserror)
        ├── html_debug.rs  # (Needs rework) HTML debugging helpers
        ├── logging.rs     # Logging setup (tracing)
        └── mod.rs
```

## Getting Started (Development)

**Prerequisites:**

* Rust toolchain (latest stable recommended): [https://rustup.rs/](https://rustup.rs/)

**Installation:**

```bash
git clone <repository-url>
cd <repository-name>
cargo build
```

## Usage

The application uses `clap` for command-line argument parsing.

```bash
# Example: Fetch and extract Item 8 for Apple's 2023 10-K
# Set log level via environment variable (e.g., info, debug, trace)
export RUST_LOG=info

cargo run -- --ticker AAPL --year 2023

# Example with date range and debug flag (Note: debug HTML generation needs rework)
export RUST_LOG=debug
cargo run -- --ticker MSFT --start-year 2022 --end-year 2023 --output ./financial_data --debug

# Other options:
# --accession-number <acc_num> # (Currently not implemented)
# --min-section-size <bytes> # Set minimum byte size for extracted section (default: 1000)
```

## Configuration

* **SEC EDGAR User-Agent:** The client *must* send a valid `User-Agent` header (e.g., `CompanyName YourName your.email@example.com`). This is currently hardcoded in `src/edgar/client.rs` - **modify it with your details before use to avoid being blocked by the SEC.**
* **Rate Limiting:** Basic rate limiting (delay between requests) is included in `src/edgar/client.rs` to comply with SEC guidelines (max 10 requests/second).

## Dependencies (Core)

* `tokio`: Asynchronous runtime
* `reqwest`: HTTP client
* `scraper`: HTML parsing and DOM querying via CSS selectors
* `regex`: Regular expressions for text matching
* `serde`, `serde_json`: Data serialization/deserialization
* `clap`: Command-line argument parsing
* `thiserror`: Error handling boilerplate
* `tracing`, `tracing-subscriber`: Logging framework
* `roxmltree`: (Added for future) XML parsing (XBRL)
* `chrono`: Date/time handling

## Implementation To-Do / Next Steps

* **Refine `find_section_boundaries` Logic:**
  * Improve robustness in finding the *correct* end marker element after the start element.
  * Handle cases where specific end markers (like Item 9) are missing, falling back to other markers (Part III, Signatures) or document structure.
  * Implement a fallback if *no* suitable end marker is found (e.g., extract up to a certain size limit or end of Part II).
* **Refine `extract_html_between` Function:**
  * Ensure accurate HTML reconstruction for the extracted slice.
  * Verify correct handling of different node types (text, elements, comments if needed).
* **Improve `is_in_toc_dom` Check:**
  * Make the ToC detection more robust; consider checking preceding sibling headings or using more sophisticated heuristics beyond just ancestor classes/IDs.
* **Add Comprehensive Tests:**
  * Write detailed unit tests for `find_section_boundaries`, `is_in_toc_dom`, and `extract_html_between` covering various HTML structures and edge cases found in real filings.
  * Adapt integration tests to validate the end-to-end DOM-based extraction.
* **Rework/Remove Debug HTML Utility:** The current `utils/html_debug.rs` relies on byte offsets. It needs to be significantly reworked to highlight DOM elements based on `ElementRef` or selectors, or be removed if not deemed essential for the DOM approach.
* **Implement XBRL Parsing:** Add logic to identify iXBRL tags within the extracted Item 8 HTML (`content_html`), extract the relevant XML fragments, and parse them using `roxmltree` to get structured financial facts. (Potentially in `src/extractors/xbrl.rs`).
* **Test with Real-World Filings:** Validate the extractor against a diverse set of 10-K filings from different companies and years.

## Contributing

(TODO: Add contribution guidelines if applicable)

## License

(TODO: Choose and add a license, e.g., MIT or Apache-2.0)
