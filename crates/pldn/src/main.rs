mod cli;

use crate::cli::{Cli, Command, CoverageArgs, SharedArgs};
use anyhow::{anyhow, Result};
use clap::Parser;
use log::info;
use noodles::sam::alignment::record::Flags;
use piledown::engine::{runtime, EngineConfig, JunctionEngine, PileEngine};
use piledown::junction::{read_junctions_tsv, JunctionRegion};
use piledown::output::{
    write_junction_stream_as_arrow, write_junction_stream_as_parquet, write_junction_stream_as_tsv,
    write_stream_as_arrow, write_stream_as_parquet, write_stream_as_tsv,
};
use piledown::region::{read_regions_tsv, PileRegion};
use piledown::types::OutputFormat;

fn main() -> Result<()> {
    // Try parsing with subcommands. If that fails (e.g., bare `pldn <bam> ...`),
    // try parsing as coverage args for backward compatibility.
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(e) => {
            // If it's a help/version request, print and exit
            if e.kind() == clap::error::ErrorKind::DisplayHelp
                || e.kind() == clap::error::ErrorKind::DisplayVersion
            {
                e.exit();
            }
            // Try parsing as bare coverage args (backward compat)
            match CoverageArgs::try_parse() {
                Ok(coverage_args) => Cli {
                    command: Command::Coverage(coverage_args),
                },
                Err(_) => {
                    // Show the original error
                    e.exit();
                }
            }
        }
    };

    match cli.command {
        Command::Coverage(args) => run_coverage(args),
        Command::Junctions(shared) => run_junctions(shared),
    }
}

fn setup_logger(verbose: &clap_verbosity_flag::Verbosity) {
    let mut logger = env_logger::Builder::from_default_env();
    if verbose.is_present() {
        logger.filter_level(verbose.log_level_filter());
    }
    logger.init();
}

fn run_coverage(args: CoverageArgs) -> Result<()> {
    let shared = args.shared;
    setup_logger(&shared.verbose);

    anyhow::ensure!(
        shared.input.exists(),
        "input file not found: {}",
        shared.input.display()
    );
    anyhow::ensure!(shared.concurrency > 0, "concurrency must be >= 1");
    if let Some(cs) = args.chunk_size {
        anyhow::ensure!(cs > 0, "chunk_size must be >= 1");
    }
    if shared.row_group_size == 0 {
        anyhow::bail!("row_group_size must be >= 1");
    }

    let exclude_flags = shared.exclude.map(Flags::from);

    let regions = if let Some(region_str) = &shared.region {
        let strand = shared
            .strand
            .ok_or_else(|| anyhow!("--strand required with --region"))?;
        vec![PileRegion::from_region_str(
            region_str,
            shared.name.clone(),
            strand,
        )?]
    } else if let Some(path) = &shared.regions_file {
        let file = std::fs::File::open(path)?;
        read_regions_tsv(file)?
    } else {
        return Err(anyhow!("provide either --region or --regions-file"));
    };

    info!("processing {} region(s)", regions.len());

    let config = EngineConfig {
        bam_path: shared.input,
        exclude_flags,
        lib_type: shared.lib_fragment_type,
        concurrency: shared.concurrency,
        index_path: shared.bam_index,
        chunk_size: args.chunk_size,
        anchor_length: shared.anchor_length,
    };

    let engine = PileEngine::new(config);
    let rt = runtime();
    rt.block_on(async {
        let stream = engine.run(regions);
        match shared.output_format {
            OutputFormat::Tsv => {
                let stdout = tokio::io::stdout();
                write_stream_as_tsv(stream, stdout).await
            }
            OutputFormat::Arrow => {
                let stdout = std::io::stdout();
                write_stream_as_arrow(stream, stdout).await
            }
            OutputFormat::Parquet => {
                let props = piledown::output::parquet_props_builder()
                    .set_max_row_group_row_count(Some(shared.row_group_size))
                    .build();
                let stdout = tokio::io::stdout();
                write_stream_as_parquet(stream, stdout, Some(props)).await
            }
        }
    })?;

    info!("Done!");
    Ok(())
}

fn run_junctions(shared: SharedArgs) -> Result<()> {
    setup_logger(&shared.verbose);

    anyhow::ensure!(
        shared.input.exists(),
        "input file not found: {}",
        shared.input.display()
    );
    anyhow::ensure!(shared.concurrency > 0, "concurrency must be >= 1");
    if shared.row_group_size == 0 {
        anyhow::bail!("row_group_size must be >= 1");
    }

    let exclude_flags = shared.exclude.map(Flags::from);

    let junctions = if let Some(region_str) = &shared.region {
        let strand = shared
            .strand
            .ok_or_else(|| anyhow!("--strand required with --region"))?;
        let parsed: noodles::core::Region = region_str
            .parse()
            .map_err(|e: noodles::core::region::ParseError| anyhow!(e))?;
        let seq = String::from_utf8(parsed.name().to_vec())?;
        let start = parsed
            .interval()
            .start()
            .ok_or_else(|| anyhow!("missing start"))?
            .get() as u64;
        let end = parsed
            .interval()
            .end()
            .ok_or_else(|| anyhow!("missing end"))?
            .get() as u64;
        vec![JunctionRegion::new(
            seq,
            start,
            end,
            shared.name.clone(),
            strand,
        )?]
    } else if let Some(path) = &shared.regions_file {
        let file = std::fs::File::open(path)?;
        read_junctions_tsv(file)?
    } else {
        return Err(anyhow!("provide either --region or --regions-file"));
    };

    info!("processing {} junction(s)", junctions.len());

    let config = EngineConfig {
        bam_path: shared.input,
        exclude_flags,
        lib_type: shared.lib_fragment_type,
        concurrency: shared.concurrency,
        index_path: shared.bam_index,
        chunk_size: None,
        anchor_length: shared.anchor_length,
    };

    let engine = JunctionEngine::new(config);
    let rt = runtime();
    rt.block_on(async {
        let stream = engine.run(junctions);
        match shared.output_format {
            OutputFormat::Tsv => {
                let stdout = tokio::io::stdout();
                write_junction_stream_as_tsv(stream, stdout).await
            }
            OutputFormat::Arrow => {
                let stdout = std::io::stdout();
                write_junction_stream_as_arrow(stream, stdout).await
            }
            OutputFormat::Parquet => {
                let props = piledown::output::parquet_props_builder()
                    .set_max_row_group_row_count(Some(shared.row_group_size))
                    .build();
                let stdout = tokio::io::stdout();
                write_junction_stream_as_parquet(stream, stdout, Some(props)).await
            }
        }
    })?;

    info!("Done!");
    Ok(())
}
