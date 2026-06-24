// SPDX-License-Identifier: UNLICENSED
// Copyright (c) 2026 GENEx Contributors
//
// genex-platform/src/alignment_engine.rs — Genomic alignment pipeline.
//
// Loads reference + query FASTA files, dispatches alignment to the real
// Smith-Waterman-Gotoh implementation, and returns scored alignment vectors.
// In production, the BC250 APU grid accelerates the DP matrix via Vulkan compute.

use crate::fasta_parser::parse_fasta_file;
use crate::smith_waterman::{smith_waterman_align, ScoreParams};

/// Result of a single alignment operation.
#[derive(Debug, Clone)]
pub struct AlignmentVector {
    /// Chromosome identifier.
    pub chromosome: String,
    /// Start position (1-based).
    pub start: u64,
    /// End position (1-based).
    pub end: u64,
    /// Alignment score.
    pub score: f64,
    /// CIGAR string representation.
    pub cigar: String,
}

/// Runs the real Smith-Waterman alignment pipeline.
///
/// 1. Parses reference and query FASTA files.
/// 2. Runs Smith-Waterman-Gotoh alignment on the first record pair.
/// 3. Returns scored alignment vectors with CIGAR strings.
///
/// # Errors
///
/// Returns an error if FASTA files cannot be read or parsed.
pub async fn run_alignment(
    reference_path: &str,
    query_path: &str,
) -> anyhow::Result<Vec<AlignmentVector>> {
    tracing::info!(
        reference = reference_path,
        query = query_path,
        "Alignment engine: parsing FASTA and running Smith-Waterman"
    );

    let ref_records = parse_fasta_file(reference_path)
        .map_err(|e| anyhow::anyhow!("Failed to parse reference FASTA: {e}"))?;
    let qry_records = parse_fasta_file(query_path)
        .map_err(|e| anyhow::anyhow!("Failed to parse query FASTA: {e}"))?;

    if ref_records.is_empty() || qry_records.is_empty() {
        tracing::warn!("One or both FASTA files are empty; returning no alignments");
        return Ok(vec![]);
    }

    let params = ScoreParams {
        match_score: 2,
        mismatch_penalty: -3,
        gap_open: -5,
        gap_extend: -2,
    };

    let min_score = 20i32;

    let mut vectors: Vec<AlignmentVector> = Vec::new();

    // Align each query against each reference (for production: shard across BC250 grid).
    for ref_record in &ref_records {
        for qry_record in &qry_records {
            let results = smith_waterman_align(
                ref_record.sequence.as_bytes(),
                qry_record.sequence.as_bytes(),
                &params,
                min_score,
            );

            for sw_result in &results {
                let start = sw_result.ref_end.saturating_sub(sw_result.alignment_length);
                vectors.push(AlignmentVector {
                    chromosome: ref_record.header.clone(),
                    start: start as u64 + 1, // 1-based
                    end: sw_result.ref_end as u64,
                    score: sw_result.max_score as f64,
                    cigar: sw_result.cigar.clone(),
                });
            }
        }
    }

    // Sort by score descending.
    vectors.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    tracing::info!(count = vectors.len(), "Alignment complete (real Smith-Waterman)");
    Ok(vectors)
}
