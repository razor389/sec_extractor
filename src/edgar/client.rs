// src/edgar/client.rs
use crate::utils::error::EdgarError;
use reqwest::header;
use std::time::Duration;
use crate::edgar::models::{CompanySubmission, FilingInfo};

// IMPORTANT: Replace with your actual details or make configurable
const EDGAR_USER_AGENT: &str = "Orot Capital Ross Granowski rgranowski@gmail.com";
// SEC asks for 10 requests/second max. Be conservative. >100ms delay.
const EDGAR_REQUEST_DELAY_MS: u64 = 150;

/// Creates a reqwest client configured for EDGAR interaction.
fn build_edgar_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .user_agent(EDGAR_USER_AGENT) // Set the required User-Agent
        // Can add more config like timeouts here
        .build()
}

/// Downloads a specific filing document from its URL.
/// Includes mandatory User-Agent and basic rate limiting.
pub async fn download_filing_doc(url: &str) -> Result<String, EdgarError> {
    let client = build_edgar_client()?; // Propagate client build error if any

    tracing::info!("Downloading document from: {}", url);
    tracing::debug!("Using User-Agent: {}", EDGAR_USER_AGENT);

    // --- Basic Rate Limiting ---
    // In a real app, use a more sophisticated approach like `governor`
    // especially if making concurrent requests.
    tokio::time::sleep(Duration::from_millis(EDGAR_REQUEST_DELAY_MS)).await;
    // --------------------------

    let response = client.get(url)
        // SEC uses various content types, but often text/html for filings
        .header(header::ACCEPT, "application/xml,text/html,text/plain,*/*")
        .send()
        .await?; // Propagates reqwest::Error as EdgarError::Network

    // Check if the request was successful (status code 2xx)
    let status = response.status();
    if !status.is_success() {
         tracing::error!("HTTP error status: {} for URL: {}", status, url);
         // Check for specific common errors
         if status == reqwest::StatusCode::FORBIDDEN {
              tracing::warn!("Received 403 Forbidden - check User-Agent and rate limits.");
              return Err(EdgarError::RateLimited);
         }
         if status == reqwest::StatusCode::NOT_FOUND {
              tracing::warn!("Received 404 Not Found for URL: {}", url);
               return Err(EdgarError::FilingDocNotFound(url.to_string()));
         }
         // Return generic HTTP error
         return Err(EdgarError::Http(status));
    }

    // Read the response body as text
    let body = response.text().await?; // Propagates reqwest::Error as EdgarError::Network
    tracing::debug!("Successfully downloaded {} bytes from {}", body.len(), url);

    Ok(body)
}

/// Gets the CIK (Central Index Key) for a ticker symbol
pub async fn get_cik_from_ticker(ticker: &str) -> Result<String, EdgarError> {
    let ticker = ticker.to_uppercase();
    let url = "https://www.sec.gov/files/company_tickers.json";
    
    let client = build_edgar_client()?;
    tokio::time::sleep(Duration::from_millis(EDGAR_REQUEST_DELAY_MS)).await;
    
    let response = client.get(url)
        .send()
        .await?;
        
    if !response.status().is_success() {
        return Err(EdgarError::Http(response.status()));
    }
    
    let json: serde_json::Value = response.json().await?;
    
    // Iterate through the company list to find the matching ticker
    for (_idx, company) in json.as_object().ok_or(EdgarError::Parse("Invalid JSON structure".to_string()))? {
        if let Some(company_ticker) = company.get("ticker") {
            if company_ticker.as_str().unwrap_or_default().to_uppercase() == ticker {
                if let Some(cik) = company.get("cik_str") {
                    // Format CIK with leading zeros to 10 digits
                    let cik_num = cik.as_u64().ok_or(EdgarError::Parse("Invalid CIK format".to_string()))?;
                    return Ok(format!("{:010}", cik_num));
                }
            }
        }
    }
    
    Err(EdgarError::Parse(format!("Could not find CIK for ticker {}", ticker)))
}

/// Fetches the company submission data for a given CIK
pub async fn get_company_submissions(cik: &str) -> Result<CompanySubmission, EdgarError> {
    let url = format!("https://data.sec.gov/submissions/CIK{}.json", cik);
    
    let client = build_edgar_client()?;
    tokio::time::sleep(Duration::from_millis(EDGAR_REQUEST_DELAY_MS)).await;
    
    let response = client.get(&url)
        .send()
        .await?;
        
    if !response.status().is_success() {
        return Err(EdgarError::Http(response.status()));
    }
    
    let submission: CompanySubmission = response.json().await?;
    Ok(submission)
}

/// Finds 10-K filings for a given ticker within a year range
pub async fn find_10k_filings(ticker: &str, start_year: Option<u32>, end_year: Option<u32>) 
    -> Result<Vec<FilingInfo>, EdgarError> 
{
    let cik = get_cik_from_ticker(ticker).await?;
    let submissions = get_company_submissions(&cik).await?;
    
    let mut filings = Vec::new();
    
    // Process recent filings
    for i in 0..submissions.filings.recent.accessionNumber.len() {
        let form = submissions.filings.recent.form.get(i)
            .ok_or_else(|| EdgarError::Parse("Missing form type".to_string()))?;
            
        // Filter for 10-K filings
        if form == "10-K" {
            let filing_date = submissions.filings.recent.filingDate.get(i)
                .ok_or_else(|| EdgarError::Parse("Missing filing date".to_string()))?;
                
            // Parse year from filing date (format: YYYY-MM-DD)
            let year = filing_date[0..4].parse::<u32>()
                .map_err(|_| EdgarError::Parse("Invalid date format".to_string()))?;
                
            // Apply year filtering if specified
            if (start_year.is_none() || year >= start_year.unwrap()) && 
               (end_year.is_none() || year <= end_year.unwrap()) {
                
                let acc_num = submissions.filings.recent.accessionNumber.get(i)
                    .ok_or_else(|| EdgarError::Parse("Missing accession number".to_string()))?;
                let primary_doc = submissions.filings.recent.primaryDocument.get(i)
                    .ok_or_else(|| EdgarError::Parse("Missing primary document".to_string()))?;
                
                filings.push(FilingInfo {
                    accession_number: acc_num.clone(),
                    filing_date: filing_date.clone(),
                    form_type: form.clone(),
                    ticker: ticker.to_uppercase(),
                    company_name: submissions.name.clone(),
                    cik: cik.clone(),
                    primary_doc: primary_doc.clone(),
                    year: Some(year),
                });
            }
        }
    }
    
    // Sort by year (newest first)
    filings.sort_by(|a, b| b.year.unwrap_or(0).cmp(&a.year.unwrap_or(0)));
    
    Ok(filings)
}