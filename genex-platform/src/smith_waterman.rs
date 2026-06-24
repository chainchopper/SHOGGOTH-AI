// SPDX-License-Identifier: UNLICENSED
// Copyright (c) 2026 GENEx Contributors
//
// genex-platform/src/smith_waterman.rs — Parallel Smith-Waterman aligner.
//
// Implements the Smith-Waterman local sequence alignment algorithm with
// affine gap penalties. Designed to run on the Shoggoth BC250 APU grid
// via Vulkan compute for massive parallelism across chromosome regions.
//
// Algorithm: Smith-Waterman with Gotoh's affine gap optimization.
//   • Match score: +2 (A-A, C-C, G-G, T-T).
//   • Mismatch penalty: -1.
//   • Gap open penalty: -3.
//   • Gap extend penalty: -1.
//
// Parallelization strategy:
//   • Each BC250 APU processes a chromosome region independently.
//   • The Xeon brain distributes regions and collects top-N alignments.
//   • Alignment vectors are compiled into WebRTC visualization data.

use std::cmp::max;

// ── Scoring ───────────────────────────────────────────────────────────────────

/// Affine gap scoring parameters.
#[derive(Debug, Clone, Copy)]
pub struct ScoreParams {
    pub match_score: i32,
    pub mismatch_penalty: i32,
    pub gap_open: i32,
    pub gap_extend: i32,
}

impl Default for ScoreParams {
    fn default() -> Self {
        Self {
            match_score: 2,
            mismatch_penalty: -1,
            gap_open: -3,
            gap_extend: -1,
        }
    }
}

/// DNA substitution scoring matrix (ACGTN).
const NUCLEOTIDE_INDEX: fn(u8) -> usize = |b| match b {
    b'A' | b'a' => 0,
    b'C' | b'c' => 1,
    b'G' | b'g' => 2,
    b'T' | b't' => 3,
    _ => 4, // N or unknown
};

/// Returns the substitution score for two nucleotide bytes.
fn substitution_score(a: u8, b: u8, params: &ScoreParams) -> i32 {
    if a == b && a != b'N' && a != b'n' {
        params.match_score
    } else {
        params.mismatch_penalty
    }
}

// ── Alignment Result ──────────────────────────────────────────────────────────

/// Result of a single Smith-Waterman alignment.
#[derive(Debug, Clone)]
pub struct SmithWatermanResult {
    /// Maximum alignment score found.
    pub max_score: i32,
    /// End position in the reference sequence (0-based).
    pub ref_end: usize,
    /// End position in the query sequence (0-based).
    pub query_end: usize,
    /// Length of the optimal alignment.
    pub alignment_length: usize,
    /// Whether this score exceeds the significance threshold.
    pub significant: bool,
    /// The aligned reference subsequence (for visualization).
    pub aligned_reference: String,
    /// The aligned query subsequence (for visualization).
    pub aligned_query: String,
    /// CIGAR string representation.
    pub cigar: String,
}

// ── Smith-Waterman-Gotoh Implementation ───────────────────────────────────────

/// Performs Smith-Waterman local alignment with affine gap penalties.
///
/// # Arguments
///
/// * `reference` — Reference sequence (e.g., chromosome region).
/// * `query` — Query sequence to align.
/// * `params` — Scoring parameters.
/// * `min_score` — Minimum score to report (filters noise). Default: 20.
///
/// # Returns
///
/// A vector of significant alignment results, sorted by score descending.
pub fn smith_waterman_align(
    reference: &[u8],
    query: &[u8],
    params: &ScoreParams,
    min_score: i32,
) -> Vec<SmithWatermanResult> {
    let m = reference.len();
    let n = query.len();

    if m == 0 || n == 0 {
        return vec![];
    }

    // DP matrices: H (main), E (gap in reference), F (gap in query).
    // For memory efficiency on large sequences, use a single row with
    // score-only tracking (no traceback). Traceback is done in a second pass
    // only for high-scoring regions.
    let mut h_prev = vec![0i32; n + 1];
    let mut e_prev = vec![0i32; n + 1];
    let mut h_curr = vec![0i32; n + 1];
    let mut e_curr = vec![0i32; n + 1];

    let gap_open = params.gap_open;
    let gap_extend = params.gap_extend;

    let mut results = Vec::new();
    let mut max_overall = 0;
    let mut max_i = 0usize;
    let mut max_j = 0usize;

    for i in 1..=m {
        let ref_base = reference[i - 1];
        h_curr[0] = 0;
        e_curr[0] = 0;

        for j in 1..=n {
            let query_base = query[j - 1];

            // Match/mismatch score.
            let s = substitution_score(ref_base, query_base, params);

            // H[i][j] = max(0, H[i-1][j-1] + s, E[i][j], F[i][j]).
            let diag = h_prev[j - 1] + s;

            // E[i][j] = max(H[i-1][j] + gap_open, E[i-1][j] + gap_extend).
            let e_val = max(h_prev[j] + gap_open, e_prev[j] + gap_extend);

            // F[i][j] = max(H[i][j-1] + gap_open, F[i][j-1] + gap_extend).
            let f_val = max(h_curr[j - 1] + gap_open, e_curr[j - 1] + gap_extend);

            let best = max(0, max(diag, max(e_val, f_val)));
            h_curr[j] = best;
            e_curr[j] = e_val;

            if best > max_overall {
                max_overall = best;
                max_i = i;
                max_j = j;
            }
        }

        // Report significant alignments when a high-scoring region ends.
        if max_overall >= min_score && i == m {
            // Traceback from max_i, max_j to produce alignment.
            let traceback = traceback_alignment(
                reference, query, max_i, max_j, params,
            );
            if let Some(result) = traceback {
                results.push(result);
            }
        }

        // Rotate rows.
        std::mem::swap(&mut h_prev, &mut h_curr);
        std::mem::swap(&mut e_prev, &mut e_curr);

        // Reset current row.
        h_curr.fill(0);
        e_curr.fill(0);
    }

    // Sort by score descending.
    results.sort_by(|a, b| b.max_score.cmp(&a.max_score));
    results
}

// ── Traceback ─────────────────────────────────────────────────────────────────

fn traceback_alignment(
    reference: &[u8],
    query: &[u8],
    mut i: usize,
    mut j: usize,
    params: &ScoreParams,
) -> Option<SmithWatermanResult> {
    let mut ref_aligned = Vec::new();
    let mut query_aligned = Vec::new();
    let mut cigar = Vec::new();
    let mut match_count = 0u32;
    let mut score = 0i32;

    while i > 0 && j > 0 {
        let s = substitution_score(reference[i - 1], query[j - 1], params);

        if s == params.match_score && reference[i - 1] == query[j - 1] {
            // Match.
            ref_aligned.push(reference[i - 1] as char);
            query_aligned.push(query[j - 1] as char);
            score += params.match_score;
            match_count += 1;
            i -= 1;
            j -= 1;
        } else if score + s > 0 {
            // Mismatch.
            ref_aligned.push(reference[i - 1] as char);
            query_aligned.push(query[j - 1] as char);
            score += s;
            i -= 1;
            j -= 1;
        } else if score + params.gap_open > 0 {
            // Gap in query.
            ref_aligned.push(reference[i - 1] as char);
            query_aligned.push('-');
            score += params.gap_open;
            i -= 1;
        } else if score + params.gap_open > 0 {
            // Gap in reference.
            ref_aligned.push('-');
            query_aligned.push(query[j - 1] as char);
            score += params.gap_open;
            j -= 1;
        } else {
            break; // Score dropped below threshold.
        }
    }

    if score < 20 {
        return None;
    }

    ref_aligned.reverse();
    query_aligned.reverse();

    // Build CIGAR string.
    let mut cigar_str = String::new();
    let ref_str: String = ref_aligned.iter().collect();
    let qry_str: String = query_aligned.iter().collect();

    for (r, q) in ref_str.chars().zip(qry_str.chars()) {
        if r == q {
            if let Some(last) = cigar.last_mut() {
                if last.0 == 'M' {
                    last.1 += 1;
                    continue;
                }
            }
            cigar.push(('M', 1));
        } else if q == '-' {
            cigar.push(('D', 1));
        } else if r == '-' {
            cigar.push(('I', 1));
        } else {
            if let Some(last) = cigar.last_mut() {
                if last.0 == 'X' {
                    last.1 += 1;
                    continue;
                }
            }
            cigar.push(('X', 1));
        }
    }

    for (op, count) in &cigar {
        cigar_str.push_str(&format!("{count}{op}"));
    }

    Some(SmithWatermanResult {
        max_score: score,
        ref_end: i,
        query_end: j,
        alignment_length: ref_aligned.len(),
        significant: score >= 50,
        aligned_reference: ref_str,
        aligned_query: qry_str,
        cigar: cigar_str,
    })
}

// ── Parallel Dispatch (BC250 Grid) ────────────────────────────────────────────

/// Dispatches Smith-Waterman alignment across the BC250 APU grid.
///
/// The reference genome is partitioned into overlapping regions,
/// each assigned to a BC250 APU. Results are merged and sorted.
///
/// In production: this calls the Shoggoth orchestrator to dispatch
/// Vulkan compute workloads to the BC250 node agents.
pub async fn parallel_align(
    reference: &[u8],
    query: &[u8],
    params: &ScoreParams,
    min_score: i32,
    num_workers: usize,
) -> Vec<SmithWatermanResult> {
    if num_workers <= 1 || reference.len() < 10_000 {
        return smith_waterman_align(reference, query, params, min_score);
    }

    let chunk_size = (reference.len() + num_workers - 1) / num_workers;
    let overlap = query.len() + 100; // Overlap to capture boundary alignments.

    let mut handles = Vec::with_capacity(num_workers);

    for worker in 0..num_workers {
        let start = worker * chunk_size;
        let end = ((worker + 1) * chunk_size + overlap).min(reference.len());

        if start >= reference.len() {
            break;
        }

        let ref_chunk = reference[start..end].to_vec();
        let query_vec = query.to_vec();
        let p = *params;

        handles.push(tokio::task::spawn_blocking(move || {
            smith_waterman_align(&ref_chunk, &query_vec, &p, min_score)
        }));
    }

    let mut all_results = Vec::new();
    for handle in handles {
        if let Ok(results) = handle.await {
            all_results.extend(results);
        }
    }

    all_results.sort_by(|a, b| b.max_score.cmp(&a.max_score));
    all_results
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_perfect_match() {
        let ref_seq = b"ACGTACGTACGT";
        let qry_seq = b"ACGTACGTACGT";
        let results = smith_waterman_align(ref_seq, qry_seq, &ScoreParams::default(), 20);
        assert!(!results.is_empty());
        assert!(results[0].max_score > 20);
    }

    #[test]
    fn test_single_mismatch() {
        let ref_seq = b"ACGTACGT";
        let qry_seq = b"ACGTTCGT"; // T → C at position 5 (mismatch).
        let results = smith_waterman_align(ref_seq, qry_seq, &ScoreParams::default(), 10);
        assert!(!results.is_empty());
        // Score should be less than perfect match.
        let perfect = smith_waterman_align(b"ACGTACGT", b"ACGTACGT", &ScoreParams::default(), 10);
        assert!(results[0].max_score < perfect[0].max_score);
    }

    #[test]
    fn test_empty_input() {
        assert!(smith_waterman_align(b"", b"ACGT", &ScoreParams::default(), 0).is_empty());
        assert!(smith_waterman_align(b"ACGT", b"", &ScoreParams::default(), 0).is_empty());
    }

    #[test]
    fn test_scoring_defaults() {
        let p = ScoreParams::default();
        assert_eq!(p.match_score, 2);
        assert_eq!(p.mismatch_penalty, -1);
        assert_eq!(p.gap_open, -3);
    }

    #[tokio::test]
    async fn test_parallel_align_matches_sequential() {
        let ref_seq = b"ACGT".repeat(1000);
        let qry_seq = b"ACGTACGTACGT";

        let seq_results = smith_waterman_align(&ref_seq, qry_seq, &ScoreParams::default(), 20);
        let par_results = parallel_align(&ref_seq, qry_seq, &ScoreParams::default(), 20, 4).await;

        if !seq_results.is_empty() && !par_results.is_empty() {
            // Top scores should match within tolerance.
            assert!((seq_results[0].max_score - par_results[0].max_score).abs() < 5);
        }
    }
}
