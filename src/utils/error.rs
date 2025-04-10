// src/utils/error.rs
#![allow(dead_code)]
use thiserror::Error;

// Define specific error types for different parts of the application
#[derive(Error, Debug)]
pub enum EdgarError {
    #[error("Network request failed: {0}")]
    Network(#[from] reqwest::Error), // Automatically convert reqwest errors

    #[error("HTTP error: {0}")]
    Http(reqwest::StatusCode), // e.g., 404 Not Found, 403 Forbidden

    #[error("SEC Rate limit likely exceeded")]
    RateLimited, // Could check for specific status codes later

    #[error("Could not find filing index for CIK {0}")]
    IndexNotFound(String),

    #[error("Could not find specified filing: {0}")]
    FilingDocNotFound(String),

    #[error("Failed to parse EDGAR response: {0}")]
    Parse(String),
}

#[derive(Error, Debug)]
pub enum ExtractError {
    #[error("Regular expression error: {0}")]
    RegexError(String),
    
    #[error("Section not found: {0}")]
    SectionNotFound(String),
    
    #[error("HTML parsing error: {0}")]
    HtmlParseError(String),
}

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("File already exists: {0}")]
    FileExists(String),
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error), // Automatically convert IO errors

    #[error("EDGAR interaction failed: {0}")]
    Edgar(#[from] EdgarError), // Automatically convert Edgar errors

    #[error("Extraction failed: {0}")]
    Extraction(#[from] ExtractError),

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("AI processing failed: {0}")]
    Ai(String),

    #[error("Data processing failed: {0}")]
    Processing(String),
}