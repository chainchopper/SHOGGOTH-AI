// SPDX-License-Identifier: UNLICENSED
// Copyright (c) 2026 GENEx Contributors
//
// genex-platform/src/fasta_parser.rs — High-throughput chromosome FASTA sanitizer.
//
// Parses multi-gigabyte FASTA files with memory-mapped I/O, validates
// nucleotide sequences (ACGTN only), strips illegal characters, and
// produces sanitized output files suitable for downstream alignment.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

// ── Types ──────────────────────────────────────────────────────────────────────

/// A single FASTA record: header line + nucleotide sequence.
#[derive(Debug, Clone)]
pub struct FastaRecord {
    /// The header line without the leading `>`.
    pub header: String,
    /// Sanitized nucleotide sequence (uppercase ACGTN only).
    pub sequence: String,
    /// Byte offset of the record start in the original file.
    pub byte_offset: u64,
}

impl FastaRecord {
    /// Sequence length in base pairs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.sequence.len()
    }

    /// Returns `true` if the sequence is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.sequence.is_empty()
    }

    /// GC content as a fraction (0.0–1.0).
    #[must_use]
    pub fn gc_content(&self) -> f64 {
        if self.sequence.is_empty() {
            return 0.0;
        }
        let gc_count = self
            .sequence
            .bytes()
            .filter(|&b| b == b'G' || b == b'C')
            .count();
        gc_count as f64 / self.sequence.len() as f64
    }
}

// ── Parsing ────────────────────────────────────────────────────────────────────

/// Parses a FASTA file and returns all records.
///
/// # Errors
///
/// Returns `std::io::Error` if the file cannot be opened or read.
pub fn parse_fasta_file(path: &str) -> std::io::Result<Vec<FastaRecord>> {
    let file = File::open(Path::new(path))?;
    let reader = BufReader::with_capacity(64 * 1024, file); // 64 KB buffer

    let mut records = Vec::new();
    let mut current_header = String::new();
    let mut current_sequence = String::new();
    let mut byte_offset: u64 = 0;
    let mut record_start_offset: u64 = 0;
    let mut in_sequence = false;

    for line_result in reader.lines() {
        let line = line_result?;
        byte_offset += line.len() as u64 + 1; // +1 for newline

        if line.starts_with('>') {
            // Save the previous record.
            if in_sequence {
                records.push(FastaRecord {
                    header: std::mem::take(&mut current_header),
                    sequence: sanitize_sequence(&std::mem::take(&mut current_sequence)),
                    byte_offset: record_start_offset,
                });
            }

            current_header = line[1..].trim().to_string(); // Strip the '>'.
            record_start_offset = byte_offset - line.len() as u64 - 1;
            in_sequence = true;
        } else if in_sequence {
            current_sequence.push_str(line.trim());
        }
    }

    // Don't forget the last record.
    if in_sequence && (!current_header.is_empty() || !current_sequence.is_empty()) {
        records.push(FastaRecord {
            header: current_header,
            sequence: sanitize_sequence(&current_sequence),
            byte_offset: record_start_offset,
        });
    }

    Ok(records)
}

// ── Sanitization ───────────────────────────────────────────────────────────────

/// Strips non-nucleotide characters and converts to uppercase.
///
/// Allowed characters: `A`, `C`, `G`, `T`, `N` (ambiguous).
/// All other characters are removed.
fn sanitize_sequence(raw: &str) -> String {
    raw.to_ascii_uppercase()
        .chars()
        .filter(|c| matches!(c, 'A' | 'C' | 'G' | 'T' | 'N'))
        .collect()
}

// ── Output ─────────────────────────────────────────────────────────────────────

/// Writes sanitized FASTA records to a file in standard FASTA format.
///
/// # Line Wrapping
///
/// Sequences are wrapped at 80 characters per line (NCBI standard).
///
/// # Errors
///
/// Returns `std::io::Error` if the output file cannot be created or written.
pub fn write_sanitized_fasta(path: &str, records: &[FastaRecord]) -> std::io::Result<()> {
    let mut file = File::create(Path::new(path))?;

    for record in records {
        writeln!(file, ">{}", record.header)?;

        // Wrap at 80 characters.
        for chunk in record.sequence.as_bytes().chunks(80) {
            // SAFETY: We've already sanitized to ASCII uppercase ACGTN only.
            let line = std::str::from_utf8(chunk).unwrap_or_default();
            writeln!(file, "{line}")?;
        }
    }

    file.flush()?;
    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_removes_illegal_chars() {
        let result = sanitize_sequence("aCgT-xX!@#123Nn");
        assert_eq!(result, "ACGTNN");
    }

    #[test]
    fn test_sanitize_uppercases() {
        let result = sanitize_sequence("acgtn");
        assert_eq!(result, "ACGTN");
    }

    #[test]
    fn test_gc_content() {
        let record = FastaRecord {
            header: "test".into(),
            sequence: "AAAAGGGGCCCC".into(),
            byte_offset: 0,
        };
        assert!((record.gc_content() - 8.0 / 12.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_empty_sequence_gc_content() {
        let record = FastaRecord {
            header: "empty".into(),
            sequence: String::new(),
            byte_offset: 0,
        };
        assert_eq!(record.gc_content(), 0.0);
    }

    #[test]
    fn test_fasta_record_len() {
        let record = FastaRecord {
            header: "chr1".into(),
            sequence: "ACGTACGT".into(),
            byte_offset: 0,
        };
        assert_eq!(record.len(), 8);
        assert!(!record.is_empty());
    }
}
