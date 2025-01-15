use crate::structs::LibFragmentType;
use clap::Parser;

/// Simple CLI to split bam files by strand.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Input alignment file
    pub input: std::path::PathBuf,

    /// Output dir for stranded alignment files
    pub outdir: std::path::PathBuf,

    /// 1 genomic region formatted as '<seq>:<start>-<stop>'
    #[arg(short, long)]
    pub region: String,

    /// Strand
    #[arg(short, long)]
    pub strand: crate::structs::Strand,

    /// Fragment library type (see samlon docs)
    #[arg(short, long, value_enum)]
    pub lib_fragment_type: LibFragmentType,

    /// u16 sam/bam flags to exclude
    #[arg(short, long)]
    pub exclude: Option<u16>,

    #[command(flatten)]
    pub verbose: clap_verbosity_flag::Verbosity,
}
