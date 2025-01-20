use core::fmt;
use fnv::FnvBuildHasher;
use indexmap::IndexMap;
use std::sync::Arc;

use anyhow::Result;
use arrow::array::{GenericStringBuilder, RecordBatch, UInt64Builder};
use arrow::datatypes::{DataType, Field, Schema};
use clap::ValueEnum;
use noodles::sam::alignment::record::Flags;
use noodles::{bam::Record, core::Region, sam::alignment::record::cigar::op::Kind};

use crate::get_strand;
use pyo3::prelude::*;

type FnvIndexMap<K, V> = IndexMap<K, V, FnvBuildHasher>;

/// Type of library preperation protocol. See [Salmon Docs](https://salmon.readthedocs.io/en/latest/library_type.html)
#[pyclass(eq, eq_int)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum LibFragmentType {
    Isf,
    Isr,
}

#[pyclass(eq, eq_int)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum Strand {
    Forward,
    Reverse,
    Either,
}
impl fmt::Display for Strand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Strand::Forward => write!(f, "+"),
            Strand::Reverse => write!(f, "-"),
            Strand::Either => write!(f, "."),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum OutputFormat {
    Tsv,
    Arrow,
    Parquet,
}

#[derive(Clone, Debug)]
pub struct Coverage {
    pub up: u64,
    pub down: u64,
}

type Pos = u64;
#[derive(Clone, Debug)]
pub struct Pile {
    pub input_bam: std::path::PathBuf,
    pub region: Region,
    pub seq: String,
    pub strand: Strand,
    pub exclude_flags: Option<Flags>,
    pub coverage: FnvIndexMap<Pos, Coverage>,
}

impl Pile {
    pub fn new(
        input_bam: std::path::PathBuf,
        region: Region,
        strand: Strand,
        exclude_flags: Option<Flags>,
    ) -> Self {
        let seq = region.name().to_string();
        let start = region.interval().start().unwrap().get() as u64;
        let end = region.interval().end().unwrap().get() as u64;
        let coverage: FnvIndexMap<u64, Coverage> = (start..=end)
            .map(|i| (i, Coverage { up: 0, down: 0 }))
            .collect();

        Self {
            input_bam,
            region,
            seq,
            strand,
            exclude_flags,
            coverage,
        }
    }

    pub fn generate(&mut self) -> Result<()> {
        let mut reader = noodles::bam::io::indexed_reader::Builder::default()
            .build_from_path(self.input_bam.clone())?;
        let header = reader.read_header()?.clone();

        let query = match reader.query(&header, &self.region) {
            Ok(q) => q,
            Err(_e) => {
                panic!()
            }
        };

        for rec in query.into_iter() {
            self.update(&rec.unwrap())?
        }
        Ok(())
    }
    pub fn to_record_batch(&self) -> Result<RecordBatch> {
        let schema = Schema::new(vec![
            Field::new("seq", DataType::Utf8, false),
            Field::new("strand", DataType::Utf8, false),
            Field::new("pos", DataType::UInt64, false),
            Field::new("up", DataType::UInt64, false),
            Field::new("down", DataType::UInt64, false),
        ]);
        let n_bases = self.coverage.len();
        let mut seq = GenericStringBuilder::<i32>::new();
        let mut strand = GenericStringBuilder::<i32>::new();
        let mut pos = UInt64Builder::with_capacity(n_bases);
        let mut up = UInt64Builder::with_capacity(n_bases);
        let mut down = UInt64Builder::with_capacity(n_bases);

        for (p, cov) in self.coverage.iter() {
            seq.append_value(self.seq.clone());
            strand.append_value(self.strand.to_string());
            pos.append_value(*p);
            up.append_value(cov.up);
            down.append_value(cov.down);
        }
        let batch = RecordBatch::try_new(
            Arc::new(schema),
            vec![
                Arc::new(seq.finish()),
                Arc::new(strand.finish()),
                Arc::new(pos.finish()),
                Arc::new(up.finish()),
                Arc::new(down.finish()),
            ],
        )
        .unwrap();
        return Ok(batch);
    }

    pub fn update(&mut self, record: &Record) -> Result<()> {
        let flags = record.flags();

        if let Some(eflags) = self.exclude_flags {
            if flags.intersects(eflags) {
                return Ok(());
            }
        }
        let strand = get_strand(LibFragmentType::Isr, flags)?;
        if (self.strand == Strand::Either) | (strand == self.strand) {
            let mut current_pos = record.alignment_start().unwrap().unwrap().get() as u64;
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
                    _ => current_pos += op.len() as u64,
                }
            }
        }
        Ok(())
    }
}

impl TryFrom<&crate::piledown::PileParams> for Pile {
    type Error = &'static str;
    fn try_from(item: &crate::piledown::PileParams) -> std::result::Result<Self, Self::Error> {
        let region = item.region.parse();
        let exclude_flags: Option<Flags> = if let Some(exclude) = item.exclude_flags {
            let exclude_flags = Flags::from(exclude);
            Some(exclude_flags)
        } else {
            None
        };
        match region {
            Ok(reg) => Ok(Pile::new(
                item.input_bam.clone(),
                reg,
                item.strand,
                exclude_flags,
            )),
            Err(_e) => Err("Could not cast PileParms to Pile"),
        }
    }
}
