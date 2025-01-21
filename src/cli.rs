use clap::Parser;
use piledown::structs::{LibFragmentType, OutputFormat, Strand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = include_str!("../assets/logo.txt"))]
pub struct Cli {
    /// Input alignment file
    pub input: std::path::PathBuf,

    /// 1 genomic region formatted as '<seq>:<start>-<stop>'
    #[arg(short, long)]
    pub region: String,

    /// Strand
    #[arg(short, long)]
    pub strand: Strand,

    /// Fragment library type (see samlon docs)
    #[arg(short, long, value_enum)]
    pub lib_fragment_type: LibFragmentType,

    /// u16 sam/bam flags to exclude
    #[arg(short, long)]
    pub exclude: Option<u16>,

    /// Output format
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Tsv)]
    pub output_format: OutputFormat,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,
}
