// SPDX-License-Identifier: UNLICENSED
// Copyright (c) 2026 GENEx Contributors
//
// genex-platform/src/main.rs — GENEx Master Processing Daemon Entry Point.
//
// GENEx is a proprietary genomics processing platform built on the Shoggoth
// Mesh Machine. It provides:
//   • High-throughput chromosome FASTA file ingestion and sanitization.
//   • Cross-platform WebRTC vector visualization of alignment results.
//   • Blockchain-anchored validation escrow marketplace with milestone tracking.
//   • ScyllaDB shard-per-core multi-threaded genomic data loading.

mod alignment_engine;
mod database_connector;
mod database_reader;
mod fasta_parser;
mod marketplace_escrow;
mod smith_waterman;

use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "genex")]
#[command(about = "GENEx — Genomics Processing Platform on Shoggoth Mesh Machine")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse and validate a chromosome FASTA file
    Ingest {
        /// Path to the FASTA file
        #[arg(short, long)]
        file: String,
        /// Output sanitized FASTA to this path
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Run alignment and generate WebRTC visualization vectors
    Align {
        /// Reference genome path
        #[arg(short, long)]
        reference: String,
        /// Query sequences path
        #[arg(short, long)]
        query: String,
    },
    /// Start the validation escrow marketplace daemon
    Escrow {
        /// Escrow contract address on the ledger
        #[arg(short, long)]
        contract: String,
    },
    /// Load genomic data into ScyllaDB with shard-per-core parallelism
    LoadDb {
        /// ScyllaDB node addresses (comma-separated)
        #[arg(short, long)]
        nodes: String,
        /// Keyspace to populate
        #[arg(short, long)]
        keyspace: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "genex=info".into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Ingest { file, output } => {
            tracing::info!(%file, "Ingesting FASTA file");
            let records = fasta_parser::parse_fasta_file(&file)?;
            tracing::info!(records = records.len(), "Parsed FASTA records");
            if let Some(out_path) = output {
                fasta_parser::write_sanitized_fasta(&out_path, &records)?;
                tracing::info!(%out_path, "Sanitized FASTA written");
            }
        }

        Commands::Align { reference, query } => {
            tracing::info!(%reference, %query, "Starting alignment pipeline");
            let vectors = alignment_engine::run_alignment(&reference, &query).await?;
            tracing::info!(vectors = vectors.len(), "Alignment vectors generated");
        }

        Commands::Escrow { contract } => {
            tracing::info!(%contract, "Starting escrow marketplace daemon");
            marketplace_escrow::run_escrow_daemon(&contract).await?;
        }

        Commands::LoadDb { nodes, keyspace } => {
            tracing::info!(%nodes, %keyspace, "Loading genomic data into ScyllaDB");
            database_connector::load_shard_per_core(&nodes, &keyspace).await?;
        }
    }

    Ok(())
}
