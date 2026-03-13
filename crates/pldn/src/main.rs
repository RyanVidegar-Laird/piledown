mod cli;

use crate::cli::Cli;
use anyhow::{anyhow, Result};
use clap::Parser;
use log::info;
use noodles::sam::alignment::record::Flags;
use piledown::engine::{runtime, EngineConfig, PileEngine};
use piledown::output::{to_record_batch, write_output};
use piledown::region::{read_regions_tsv, PileRegion};

fn main() -> Result<()> {
    let cli = Cli::parse();

    let mut logger = env_logger::Builder::from_default_env();
    if cli.verbose.is_present() {
        logger.filter_level(cli.verbose.log_level_filter());
    }
    logger.init();

    anyhow::ensure!(
        cli.input.exists(),
        "input file not found: {}",
        cli.input.display()
    );

    let exclude_flags = cli.exclude.map(Flags::from);

    // Build region list
    let regions = if let Some(region_str) = &cli.region {
        let strand = cli
            .strand
            .ok_or_else(|| anyhow!("--strand required with --region"))?;
        vec![PileRegion::from_region_str(
            region_str,
            cli.name.clone(),
            strand,
        )?]
    } else if let Some(path) = &cli.regions_file {
        let file = std::fs::File::open(path)?;
        read_regions_tsv(file)?
    } else {
        return Err(anyhow!("provide either --region or --regions-file"));
    };

    info!("processing {} region(s)", regions.len());

    let config = EngineConfig {
        bam_path: cli.input,
        exclude_flags,
        lib_type: cli.lib_fragment_type,
        concurrency: cli.concurrency,
        index_path: cli.bam_index,
        chunk_size: None,
    };

    let engine = PileEngine::new(config);
    let rt = runtime();

    let stdout = std::io::stdout();

    // Collect all results from the stream, then output
    let results: Vec<_> = rt.block_on(async {
        use futures::stream::StreamExt;
        let stream = std::pin::pin!(engine.run(regions));
        stream
            .collect::<Vec<Result<_>>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>>>()
    })?;

    if cli.output_format == piledown::types::OutputFormat::Tsv {
        for (i, (region, map)) in results.into_iter().enumerate() {
            let batch = to_record_batch(region, map)?;
            write_output(&batch, cli.output_format, &stdout, i == 0)?;
        }
    } else {
        let batches: Vec<_> = results
            .into_iter()
            .map(|(r, m)| to_record_batch(r, m))
            .collect::<Result<Vec<_>>>()?;
        if batches.is_empty() {
            return Err(anyhow!("no regions produced output"));
        }
        let combined = arrow::compute::concat_batches(&batches[0].schema(), &batches)?;
        write_output(&combined, cli.output_format, stdout, true)?;
    }

    info!("Done!");
    Ok(())
}
