// src/edgar/models.rs
#![allow(dead_code, non_snake_case)]
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Structure representing the EDGAR company submission index
/// Example: https://data.sec.gov/submissions/CIK0000320193.json
#[derive(Debug, Deserialize)]
pub struct CompanySubmission {
    pub cik: String,
    pub entityType: String,
    pub sic: String,
    pub sicDescription: String,
    pub insiderTransactionForOwnerExists: u8,
    pub insiderTransactionForIssuerExists: u8,
    pub name: String,
    pub tickers: Vec<String>,
    pub exchanges: Vec<String>,
    pub ein: Option<String>,
    pub description: Option<String>,
    pub website: Option<String>,
    pub investorWebsite: Option<String>,
    pub category: String,
    pub fiscalYearEnd: String,
    pub stateOfIncorporation: String,
    pub stateOfIncorporationDescription: String,
    pub addresses: HashMap<String, Address>,
    pub phone: String,
    pub flags: String,
    pub formerNames: Vec<FormerName>,
    pub filings: Filings,
}

#[derive(Debug, Deserialize)]
pub struct Address {
    pub street1: Option<String>,
    pub street2: Option<String>,
    pub city: String,
    pub stateOrCountry: String,
    pub zipCode: String,
    pub stateOrCountryDescription: String,
}

#[derive(Debug, Deserialize)]
pub struct FormerName {
    pub name: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Deserialize)]
pub struct Filings {
    pub recent: FilingsList,
    pub files: Vec<FilingFile>,
}

#[derive(Debug, Deserialize)]
pub struct FilingFile {
    pub name: String,
    pub filingCount: u32,
    pub filingFrom: String,
    pub filingTo: String,
}

#[derive(Debug, Deserialize)]
pub struct FilingsList {
    pub accessionNumber: Vec<String>,
    pub filingDate: Vec<String>,
    pub reportDate: Vec<String>,
    pub acceptanceDateTime: Vec<String>,
    pub act: Vec<String>,
    pub form: Vec<String>,
    pub fileNumber: Vec<String>,
    pub filmNumber: Vec<String>,
    pub items: Vec<String>,
    pub size: Vec<u64>,
    pub isXBRL: Vec<u8>,
    pub isInlineXBRL: Vec<u8>,
    pub primaryDocument: Vec<String>,
    pub primaryDocDescription: Vec<String>,
}

/// Simple struct representing a specific filing we want to process
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilingInfo {
    pub accession_number: String,
    pub filing_date: String,
    pub form_type: String, 
    pub ticker: String,
    pub company_name: String,
    pub cik: String,
    pub primary_doc: String,
    pub year: Option<u32>, // Fiscal year of the report
}

impl FilingInfo {
    /// Constructs the URL to access the primary document of this filing
    pub fn primary_doc_url(&self) -> String {
        let acc_no_dashes = self.accession_number.replace("-", "");
        format!(
            "https://www.sec.gov/Archives/edgar/data/{}/{}/{}",
            self.cik, acc_no_dashes, self.primary_doc
        )
    }
}