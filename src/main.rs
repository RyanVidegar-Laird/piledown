mod cli;

use crate::cli::*;
use anyhow::Result;
use clap::Parser;
use core::panic;
use log::{debug, error, info};
use noodles::{bam, core::Region, sam::alignment::record::Flags};
use piledown::structs::*;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut logger = env_logger::Builder::from_default_env();
    match cli.verbose.is_present() {
        true => {
            logger.filter_level(cli.verbose.log_level_filter());
        }
        false => {}
    }
    logger.init();

    info!("input: {:?}", cli.input.clone());
    let mut reader =
        bam::io::indexed_reader::Builder::default().build_from_path(cli.input.clone())?;
    let header = reader.read_header()?.clone();

    let exclude_flags: Option<Flags> = if let Some(exclude) = cli.exclude {
        let exclude_flags = Flags::from(exclude);
        info!("excluding reads matching any: {:?}", exclude_flags);
        Some(exclude_flags)
    } else {
        None
    };

    let region: Region = cli.region.parse()?;

    debug!("instantiating Pile");
    let mut pile = Pile::init(region.clone(), cli.strand, exclude_flags);

    let query = match reader.query(&header, &region) {
        Ok(q) => {
            info!("querying reads in: {}", region);
            q
        }
        Err(e) => {
            error!("{e}");
            panic!()
        }
    };

    debug!("iterating over queried alignment records");
    query.into_iter().for_each(|rec| pile.update(&rec.unwrap()));

    let mut writer = csv::Writer::from_writer(std::io::stdout());

    debug!("writing csv to stdout");
    writer.write_record(&["seq", "pos", "strand", "up", "down"])?;
    for (pos, cov) in pile.coverage.iter() {
        writer.serialize((pile.seq.clone(), pos, pile.strand, cov.up, cov.down))?
    }

    writer.flush()?;
    info!("Done!");
    Ok(())
}
