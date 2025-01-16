use std::collections::HashMap;

use clap::ValueEnum;
use log::debug;
use noodles::sam::alignment::record::Flags;
use noodles::{bam::Record, core::Region, sam::alignment::record::cigar::op::Kind};

use crate::get_strand;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LibFragmentType {
    Isf,
    Isr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum, serde::Serialize)]
pub enum Strand {
    Forward,
    Reverse,
    Either,
}

#[derive(Clone, Debug)]
pub struct Coverage {
    pub up: usize,
    pub down: usize,
}

type Pos = usize;

#[derive(Clone, Debug)]
pub struct Pile {
    pub seq: String,
    pub strand: Strand,
    pub exclude_flags: Option<Flags>,
    pub coverage: HashMap<Pos, Coverage>,
}

impl Pile {
    pub fn init(region: Region, strand: Strand, exclude_flags: Option<Flags>) -> Self {
        let seq = region.name().to_string();
        let start = region.interval().start().unwrap().get();
        let end = region.interval().end().unwrap().get();
        let coverage: HashMap<usize, Coverage> = (start..=end)
            .map(|i| (i, Coverage { up: 0, down: 0 }))
            .collect();

        Self {
            seq,
            strand,
            exclude_flags,
            coverage,
        }
    }

    pub fn update(&mut self, record: &Record) {
        let flags = record.flags();

        if let Some(eflags) = self.exclude_flags {
            match flags.intersects(eflags) {
                true => return,
                false => {}
            }
        }
        let strand = get_strand(LibFragmentType::Isr, flags);
        match strand {
            Ok(strand) => {
                if (self.strand == Strand::Either) | (strand == self.strand) {
                    let mut current_pos = record.alignment_start().unwrap().unwrap().get();
                    for op in record.cigar().iter() {
                        let op = op.unwrap();
                        match op.kind() {
                            Kind::Match => {
                                for _ in 1..=op.len() {
                                    if let Some(bp) = self.coverage.get_mut(&current_pos) {
                                        bp.up += 1;
                                    }
                                    current_pos += 1;
                                }
                            }
                            Kind::Skip => {
                                for _ in 1..=op.len() {
                                    if let Some(bp) = self.coverage.get_mut(&current_pos) {
                                        bp.down += 1;
                                    }
                                    current_pos += 1;
                                }
                            }
                            _ => current_pos += op.len(),
                        }
                    }
                } else {
                    return;
                }
            }
            Err(err) => {
                debug!("{}", err);
            }
        }
    }
}
