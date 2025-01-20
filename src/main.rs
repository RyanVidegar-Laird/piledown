mod cli;

use crate::cli::*;
use anyhow::Result;
use clap::Parser;
use log::{debug, info};
use noodles::{core::Region, sam::alignment::record::Flags};
use piledown::structs::*;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut logger = env_logger::Builder::from_default_env();
    if cli.verbose.is_present() {
        logger.filter_level(cli.verbose.log_level_filter());
    }
    logger.init();
    info!("input: {:?}", cli.input.clone());

    let exclude_flags: Option<Flags> = if let Some(exclude) = cli.exclude {
        let exclude_flags = Flags::from(exclude);
        info!("excluding reads matching any: {:?}", exclude_flags);
        Some(exclude_flags)
    } else {
        None
    };

    let region: Region = cli.region.parse()?;
    debug!("instantiating Pile");
    let mut pile = Pile::new(cli.input.clone(), region.clone(), cli.strand, exclude_flags);
    pile.generate()?;

    let format = cli.output_format;
    pile.write(format)?;

    info!("Done!");
    Ok(())
}
