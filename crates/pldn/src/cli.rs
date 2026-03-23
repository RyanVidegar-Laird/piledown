use clap::Parser;
use piledown::types::{LibFragmentType, OutputFormat, Strand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = include_str!("../assets/logo.txt"))]
pub struct Cli {
    /// Input alignment file (indexed BAM)
    pub input: std::path::PathBuf,

    /// Single region: '<seq>:<start>-<stop>'
    #[arg(short, long, group = "region_input")]
    pub region: Option<String>,

    /// TSV file with columns: seq, start, end, name, strand
    #[arg(long, group = "region_input")]
    pub regions_file: Option<std::path::PathBuf>,

    /// Strand (required with --region, ignored with --regions-file)
    #[arg(short, long)]
    pub strand: Option<Strand>,

    /// Region name (used with --region)
    #[arg(short, long, default_value = "region")]
    pub name: String,

    /// Fragment library type (see salmon docs)
    #[arg(short, long, value_enum)]
    pub lib_fragment_type: LibFragmentType,

    /// SAM flag bits to exclude (e.g., 4=unmapped, 256=secondary, 512=failed QC, 1024=duplicate)
    #[arg(short, long)]
    pub exclude: Option<u16>,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Tsv)]
    pub output_format: OutputFormat,

    /// Path to BAM index file (.bai). If not specified, tries <bam>.bam.bai then <stem>.bai.
    #[arg(long)]
    pub bam_index: Option<std::path::PathBuf>,

    /// Maximum positions per output batch (splits large regions)
    #[arg(long)]
    pub chunk_size: Option<usize>,

    /// Parquet row group size (default: 1000000). Only used with --output-format parquet.
    #[arg(long, default_value_t = 1_000_000)]
    pub row_group_size: usize,

    /// Minimum matched bases flanking a junction for coverage to count (default: 0, no filtering)
    #[arg(long, default_value_t = 0)]
    pub anchor_length: u64,

    /// Max concurrent region queries
    #[arg(long, default_value_t = 4)]
    pub concurrency: usize,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,
}
