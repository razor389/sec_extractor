# SEC Filing Item 8 Extractor (Rust)

## Overview

This project aims to automate the extraction and parsing of "Item 8. Financial Statements and Supplementary Data" from SEC 10-K filings for publicly traded companies.

The primary goal is to fetch relevant filings from the SEC EDGAR database, accurately identify and isolate the Item 8 section within the HTML/text document, and break down its contents (including text, tables, and potentially cross-referenced XBRL data) into manageable chunks suitable for analysis by Large Language Models (LLMs).

This addresses the challenge of inconsistent formatting and XBRL tagging across different companies and filing periods, which makes purely programmatic extraction difficult and requires significant manual configuration.

## Key Features (Planned)

* **EDGAR Interaction:** Fetches company filing indexes and specific filing documents (HTML, XBRL) via the SEC EDGAR API, adhering to rate limits and user-agent requirements.
* **Item 8 Extraction:** Implements strategies (Regex, ToC parsing, structural analysis) to reliably locate the boundaries of Item 8 within filing documents.
* **Content Parsing:**
  * Extracts tables from Item 8 HTML into structured formats.
  * Parses relevant financial facts from associated XBRL instance documents.
* **AI Integration:**
  * Chunks extracted Item 8 text and tables into sizes suitable for LLM context windows.
  * Uses configurable prompts to guide LLMs (e.g., OpenAI, Anthropic) in extracting specific financial data, summaries, and classifications from chunks.
  * Abstracts API interaction with different AI providers.
* **Data Processing:**
  * Merges results from LLM calls, table extraction, and XBRL parsing.
  * Normalizes data formats (numbers, dates).
  * Performs basic validation checks (e.g., simple accounting equation checks).
* **Persistence:** Caches downloaded filings and intermediate/final results to optimize performance and reduce API calls.
* **Output:** Generates structured output (e.g., JSON) containing the parsed Item 8 data.

## Project Structure

src/
├── main.rs            # Entry point and CLI handling
├── edgar/             # SEC EDGAR API interaction
│   ├── mod.rs
│   ├── client.rs      # HTTP client with rate limiting
│   └── models.rs      # EDGAR data models (submissions, filings)
├── extractors/        # Content extraction logic
│   ├── mod.rs
│   ├── section.rs     # Item 8 boundary detection
│   ├── table.rs       # Table extraction from HTML
│   └── xbrl.rs        # XBRL data processing
├── ai/                # AI (LLM) integration
│   ├── mod.rs
│   ├── client.rs      # API client for AI providers
│   ├── prompts.rs     # Specialized prompts for financial data
│   └── chunking.rs    # Text chunking strategies
├── processors/        # Data processors
│   ├── mod.rs
│   ├── normalize.rs   # Data normalization (numbers, dates)
│   ├── validate.rs    # Validation rules and checks
│   └── merge.rs       # Result merging (LLM, tables, XBRL)
├── storage/           # Persistence layer
│   ├── mod.rs
│   ├── cache.rs       # Caching logic (files, LLM responses)
│   └── output.rs      # Output formatting (JSON, CSV, etc.)
└── utils/             # Utility functions
    ├── mod.rs
    ├── logging.rs     # Logging setup (tracing)
    └── error.rs       # Custom error handling (thiserror)
Cargo.toml
README.md

## Getting Started (Development)

**Prerequisites:**

* Rust toolchain (latest stable recommended): [https://rustup.rs/](https://rustup.rs/)
* Access to an LLM API (e.g., OpenAI) and an API key.

**Installation:**

```bash
git clone <repository-url>
cd <repository-name>
cargo build
```

## Usage (Planned)

```bash
# Example: Fetch Item 8 for Apple's 2023 10-K
# (Requires environment variables for API keys)
export RUST_LOG=info # Set log level (debug, info, warn, error)
# export OPENAI_API_KEY="your_key_here" # Example for OpenAI

cargo run -- --ticker AAPL --year 2023

# Other potential options:
# cargo run -- --ticker MSFT --start-year 2020 --end-year 2023
# cargo run -- --accession-number 0000320193-23-000106 # Specific filing
# cargo run -- --ticker GOOG --output results.json
```

**Note:** This project is under active development. The CLI interface and functionality are subject to change.

## Configuration

* **SEC EDGAR User-Agent:** The client *must* send a valid `User-Agent` header identifying your application to EDGAR (e.g., `CompanyName YourName your.email@example.com`). This is currently hardcoded in `edgar/client.rs` but should be configurable. **Failure to do so will result in being blocked by the SEC.**
* **LLM API Key:** Provide the API key for your chosen LLM provider via environment variables (e.g., `OPENAI_API_KEY`). Secure handling is essential.
* **Rate Limiting:** The EDGAR client includes basic rate limiting (delay between requests) to comply with SEC guidelines (max 10 requests/second).

## Dependencies (Core)

* `tokio`: Asynchronous runtime
* `reqwest`: HTTP client
* `serde`, `serde_json`: Data serialization/deserialization
* `clap`: Command-line argument parsing
* `thiserror`: Error handling boilerplate
* `tracing`, `tracing-subscriber`: Logging framework
* `scraper`: HTML parsing (for extraction)
* *(Potential XBRL libraries TBD)*
* *(LLM provider SDKs, e.g., `async-openai`)*

## Contributing

Contributions are welcome! Please feel free to open an issue or submit a pull request. (Add more specific guidelines later if needed).

## License

TODO
