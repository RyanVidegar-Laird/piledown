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

    /// u16 sam/bam flags to exclude
    #[arg(short, long)]
    pub exclude: Option<u16>,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Tsv)]
    pub output_format: OutputFormat,

    /// Max concurrent region queries
    #[arg(long, default_value_t = 4)]
    pub concurrency: usize,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,
}
