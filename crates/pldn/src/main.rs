mod cli;

use crate::cli::Cli;
use anyhow::{anyhow, Result};
use clap::Parser;
use log::info;
use noodles::sam::alignment::record::Flags;
use piledown::engine::{runtime, EngineConfig, PileEngine};
use piledown::output::{write_stream_as_arrow, write_stream_as_parquet, write_stream_as_tsv};
use piledown::region::{read_regions_tsv, PileRegion};
use piledown::types::OutputFormat;

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
        chunk_size: cli.chunk_size,
    };

    let engine = PileEngine::new(config);
    let rt = runtime();
    rt.block_on(async {
        let stream = engine.run(regions);
        match cli.output_format {
            OutputFormat::Tsv => {
                let stdout = tokio::io::stdout();
                write_stream_as_tsv(stream, stdout).await
            }
            OutputFormat::Arrow => {
                let stdout = std::io::stdout();
                write_stream_as_arrow(stream, stdout).await
            }
            OutputFormat::Parquet => {
                use parquet::basic::{Compression, Encoding};
                use parquet::file::properties::WriterProperties;
                use parquet::schema::types::ColumnPath;

                let props = WriterProperties::builder()
                    .set_writer_version(parquet::file::properties::WriterVersion::PARQUET_2_0)
                    .set_column_encoding(ColumnPath::from("pos"), Encoding::DELTA_BINARY_PACKED)
                    .set_column_encoding(ColumnPath::from("up"), Encoding::DELTA_BINARY_PACKED)
                    .set_column_encoding(ColumnPath::from("down"), Encoding::DELTA_BINARY_PACKED)
                    .set_compression(Compression::SNAPPY)
                    .set_max_row_group_size(cli.row_group_size)
                    .build();
                let stdout = tokio::io::stdout();
                write_stream_as_parquet(stream, stdout, Some(props)).await
            }
        }
    })?;

    info!("Done!");
    Ok(())
}
