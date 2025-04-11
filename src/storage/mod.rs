// src/storage/mod.rs
use std::fs;
use std::path::{Path, PathBuf};
use crate::extractors::section::ExtractedSection;
use crate::utils::error::StorageError;
use std::io::Write;

pub struct StorageManager {
    base_dir: PathBuf,
}

impl StorageManager {
    /// Creates a new StorageManager with the specified base directory
    pub fn new<P: AsRef<Path>>(base_dir: P) -> Result<Self, StorageError> {
        let base_path = base_dir.as_ref().to_path_buf();

        // Create the base directory if it doesn't exist
        if !base_path.exists() {
            fs::create_dir_all(&base_path)
                .map_err(StorageError::IoError)?; // Use map_err for cleaner conversion
        }

        Ok(Self { base_dir: base_path })
    }

    /// Saves the extracted section to a file
    pub fn save_section(&self, section: &ExtractedSection) -> Result<PathBuf, StorageError> {
        // Create a directory structure like: /base_dir/ticker/year/
        let target_dir = self.base_dir
            .join(&section.ticker.to_uppercase())
            .join(section.filing_year.to_string());

        // Create the directories if they don't exist
        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)
                .map_err(StorageError::IoError)?;
        }

        // Create a filename for the section
        let filename = format!("{}_{}_Item8.html",
                               section.ticker.to_uppercase(),
                               section.filing_year);

        let file_path = target_dir.join(filename);

        // Write the section content to the file
        let mut file = fs::File::create(&file_path)
            .map_err(StorageError::IoError)?;

        // *** Ensure this uses the correct field name ***
        file.write_all(section.content_html.as_bytes()) // <<< Updated field name
            .map_err(StorageError::IoError)?;

        tracing::info!("Saved section to {}", file_path.display());

        Ok(file_path)
    }

    /// Saves metadata about the section in JSON format
    pub fn save_section_metadata(&self, section: &ExtractedSection) -> Result<PathBuf, StorageError> {
        // Create a directory structure like: /base_dir/ticker/year/
        let target_dir = self.base_dir
            .join(&section.ticker.to_uppercase())
            .join(section.filing_year.to_string());

        // Create the directories if they don't exist
        if !target_dir.exists() {
            fs::create_dir_all(&target_dir)
                 .map_err(StorageError::IoError)?;
        }

        // Create a filename for the metadata
        let filename = format!("{}_{}_Item8_meta.json",
                              section.ticker.to_uppercase(),
                              section.filing_year);

        let file_path = target_dir.join(filename);

        // Create metadata structure
        let metadata = serde_json::json!({
            "ticker": section.ticker,
            "company_name": section.company_name,
            "filing_year": section.filing_year,
            "section_name": section.section_name,
            "section_title": section.section_title,
            // *** Ensure this uses the correct field name ***
            "content_length": section.content_html.len(), // <<< Updated field name
            "extraction_timestamp": chrono::Utc::now().to_rfc3339(),
        });

        // Write the metadata to the file
        let metadata_str = serde_json::to_string_pretty(&metadata)
            .map_err(|e| StorageError::SerializationError(e.to_string()))?;

        fs::write(&file_path, metadata_str)
            .map_err(StorageError::IoError)?;

        tracing::info!("Saved metadata to {}", file_path.display());

        Ok(file_path)
    }
}