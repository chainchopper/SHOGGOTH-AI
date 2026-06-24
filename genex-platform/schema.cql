-- ============================================================================
-- GENEx Platform — ScyllaDB Schema
-- Shoggoth Mesh Machine — Genomic Data Fabric
-- ============================================================================
--
-- Keyspace: genex
-- Replication: NetworkTopologyStrategy, 3 replicas across shoggoth-dc1
-- Target: Dual Xeon 6240 (72 threads, 512 GB RAM), ScyllaDB 6.2+
--
-- Schema design principles:
--   • Partition keys distribute evenly across all 72 CPU shards.
--   • Clustering keys enable range scans (chromosome position queries).
--   • COMPACT STORAGE for high-density write path on ScyllaDB.
--   • Time-To-Live (TTL) on validation results for automatic expiry.

-- ── Keyspace ──────────────────────────────────────────────────────────────────

CREATE KEYSPACE IF NOT EXISTS genex
WITH replication = {
    'class': 'NetworkTopologyStrategy',
    'shoggoth-dc1': 3
}
AND durable_writes = true;

USE genex;

-- ── Core Tables ───────────────────────────────────────────────────────────────

-- Primary sequence store: one row per chromosome position.
-- Partitioned by chromosome for even shard distribution.
-- Supports: "SELECT * FROM sequences WHERE chromosome = ? AND position >= ? AND position <= ?"
CREATE TABLE IF NOT EXISTS sequences (
    chromosome      text,
    position        bigint,
    reference_base  text,          -- A, C, G, T, N
    variant_base    text,          -- Alternate allele (NULL if reference)
    quality_score   float,         -- Phred quality score (0-60)
    read_depth      int,           -- Coverage depth at this position
    source_file     text,          -- Original FASTA/FASTQ file identifier
    ingested_at     timestamp,     -- When this record was loaded
    PRIMARY KEY (chromosome, position)
) WITH compaction = {
    'class': 'LeveledCompactionStrategy',
    'sstable_size_in_mb': 256
};

-- Alignment results indexed by sequence hash for fast lookup.
CREATE TABLE IF NOT EXISTS alignments (
    alignment_id    uuid,
    chromosome      text,
    start_pos       bigint,
    end_pos         bigint,
    query_sequence  text,          -- Truncated query (first 200 bp)
    cigar_string    text,          -- Compact Idiosyncratic Gapped Alignment Report
    mapping_quality int,           -- MAPQ score
    alignment_score float,
    created_at      timestamp,
    PRIMARY KEY (chromosome, start_pos, alignment_id)
) WITH CLUSTERING ORDER BY (start_pos ASC, alignment_id ASC);

-- ── Escrow Marketplace Tables ─────────────────────────────────────────────────

-- Active validation escrow contracts.
CREATE TABLE IF NOT EXISTS escrow_contracts (
    contract_id         uuid,
    researcher_addr     text,          -- Blockchain address of claimant
    validator_addr      text,          -- Counter-party validator address
    chromosome          text,          -- Target chromosome for this contract
    region_start        bigint,        -- Genomic region start (1-based)
    region_end          bigint,        -- Genomic region end (inclusive)
    stake_amount        bigint,        -- Amount staked (in platform tokens)
    milestone_count     int,           -- Total milestones
    milestones_done     int,           -- Completed milestones
    challenged          boolean,       -- Whether contract is challenged
    created_at          timestamp,
    expires_at          timestamp,     -- Auto-expiry for unchallenged contracts
    PRIMARY KEY (contract_id)
);

-- Milestone progress tracked per contract.
CREATE TABLE IF NOT EXISTS escrow_milestones (
    contract_id     uuid,
    milestone_idx   int,               -- 0-based milestone index
    status          text,              -- 'pending' | 'submitted' | 'verified' | 'rejected'
    submitted_at    timestamp,
    verified_at     timestamp,
    verifier_node   text,              -- Which Shoggoth node verified
    proof_hash      text,              -- blake3 hash of the verification proof
    PRIMARY KEY (contract_id, milestone_idx)
);

-- ── FASTA Ingestion Audit ─────────────────────────────────────────────────────

-- Tracks every ingested FASTA file for reproducibility.
CREATE TABLE IF NOT EXISTS fasta_ingestion_log (
    file_path       text,
    ingested_at     timestamp,
    record_count    bigint,            -- Number of sequences parsed
    total_bp        bigint,            -- Total base pairs ingested
    gc_content      float,             -- Overall GC content
    sanitized       boolean,           -- Whether illegal characters were stripped
    checksum_blake3 text,              -- Content hash for integrity
    node_id         text,              -- Which node performed ingestion
    PRIMARY KEY (file_path, ingested_at)
) WITH CLUSTERING ORDER BY (ingested_at DESC);

-- ── Secondary Indexes ─────────────────────────────────────────────────────────

-- Index on variant presence for GWAS-style queries.
CREATE INDEX IF NOT EXISTS idx_variants
ON sequences (variant_base);

-- Index on quality for filtering low-confidence calls.
CREATE INDEX IF NOT EXISTS idx_quality
ON sequences (quality_score);

-- ── Materialized View: Variants Only ──────────────────────────────────────────

CREATE MATERIALIZED VIEW IF NOT EXISTS variants_only AS
SELECT chromosome, position, reference_base, variant_base, quality_score
FROM sequences
WHERE variant_base IS NOT NULL
  AND quality_score >= 30
PRIMARY KEY (chromosome, position);
