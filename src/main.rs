mod cli;

use crate::cli::*;
use anyhow::{anyhow, Result};
use clap::Parser;
use core::panic;
use log::{debug, error, info};
use noodles::{bam, core::Region, sam::alignment::record::Flags};
use piledown::structs::*;

#[macro_use]
extern crate lazy_static;

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
    let mut piles = Pile::init(region.clone(), cli.strand);
    let keep_strand = cli.strand;

    info!("querying reads in: {}", region);
    let query = match reader.query(&header, &region) {
        Ok(q) => q,
        Err(e) => {
            error!("{e}");
            panic!()
        }
    };

    for res in query {
        let record = res.unwrap();
        let flags = record.flags();

        if let Some(eflags) = exclude_flags {
            match flags.intersects(eflags) {
                true => continue,
                false => {}
            }
        }
        let strand = get_strand(LibFragmentType::Isr, flags);
        match strand {
            Ok(strand) => {
                if strand == keep_strand {
                    piles.update(&record);
                } else {
                    continue;
                }
            }
            Err(err) => {
                debug!("{}", err);
            }
        }
    }

    let mut writer = csv::Writer::from_writer(std::io::stdout());

    writer.write_record(&["seq", "pos", "strand", "up", "down"])?;
    for (pos, cov) in piles.coverage.iter() {
        writer.serialize((piles.seq.clone(), pos, piles.strand, cov.up, cov.down))?
    }

    writer.flush()?;
    Ok(())
}

fn get_strand(lib: LibFragmentType, flags: Flags) -> Result<Strand> {
    if !flags.is_segmented() | !flags.is_properly_segmented() {
        return Err(anyhow!("not enough info to determine strand"));
    }

    // These bitflags are known at compile time, but hardcoding them is less
    // reader friendly. Instead, use lazy_static to only eval them once during runtime
    lazy_static! {

        //forward read flags for ISR
        static ref ISR_F1_FLAGS: Flags = Flags::REVERSE_COMPLEMENTED | Flags::FIRST_SEGMENT;
        static ref ISR_F2_FLAGS: Flags = Flags::MATE_REVERSE_COMPLEMENTED | Flags::LAST_SEGMENT;

        // reverse read flags for ISR
        static ref ISR_R1_FLAGS: Flags = Flags::FIRST_SEGMENT | Flags::MATE_REVERSE_COMPLEMENTED;
        static ref ISR_R2_FLAGS: Flags = Flags::REVERSE_COMPLEMENTED | Flags::LAST_SEGMENT;
    }

    match lib {
        LibFragmentType::Isr => {
            if flags.contains(*ISR_F1_FLAGS) | flags.contains(*ISR_F2_FLAGS) {
                Ok(Strand::Forward)
            } else if flags.contains(*ISR_R1_FLAGS) | flags.contains(*ISR_R2_FLAGS) {
                Ok(Strand::Reverse)
            } else {
                panic!("Unexpected flag sets: {:?}", flags);
            }
        }
        LibFragmentType::Isf => todo!(),
    }
}
