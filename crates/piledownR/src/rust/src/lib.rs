#![allow(non_snake_case)]

use extendr_api::prelude::*;

use std::path::PathBuf;

use anyhow::Result;
use arrow::ffi_stream::FFI_ArrowArrayStream;
use noodles::sam::alignment::record::Flags;

use piledown::coverage::CoverageMap;
use piledown::engine::{runtime, EngineConfig, PileEngine};
use piledown::output::to_record_batch;
use piledown::region::PileRegion;
use piledown::types::{LibFragmentType, Strand};

fn parse_strand(s: &str) -> Result<Strand> {
    match s.to_lowercase().as_str() {
        "forward" | "+" => Ok(Strand::Forward),
        "reverse" | "-" => Ok(Strand::Reverse),
        "either" | "." => Ok(Strand::Either),
        _ => anyhow::bail!(
            "invalid strand: '{}'. Expected: forward, reverse, either",
            s
        ),
    }
}

fn parse_lib_type(s: &str) -> Result<LibFragmentType> {
    match s.to_lowercase().as_str() {
        "isr" => Ok(LibFragmentType::Isr),
        "isf" => Ok(LibFragmentType::Isf),
        _ => anyhow::bail!("invalid lib_fragment_type: '{}'. Expected: isr, isf", s),
    }
}

/// Export Arrow RecordBatches to R via the nanoarrow pointer-move pattern.
fn export_batches_to_r(batches: Vec<arrow::array::RecordBatch>) -> extendr_api::Result<Robj> {
    if batches.is_empty() {
        return Err(Error::Other("no regions produced output".into()));
    }

    let schema = batches[0].schema();
    let reader = arrow::record_batch::RecordBatchIterator::new(batches.into_iter().map(Ok), schema);

    let reader: Box<dyn arrow::record_batch::RecordBatchReader + Send> = Box::new(reader);
    let mut stream = FFI_ArrowArrayStream::new(reader);
    let stream_ptr = (&mut stream) as *mut FFI_ArrowArrayStream as usize;

    let allocated = R!("nanoarrow::nanoarrow_allocate_array_stream()")
        .map_err(|e| Error::Other(format!("failed to allocate nanoarrow stream: {e}")))?;
    R!("nanoarrow::nanoarrow_pointer_move({{stream_ptr.to_string()}}, {{&allocated}})")
        .map_err(|e| Error::Other(format!("failed to move stream pointer: {e}")))?;

    Ok(allocated)
}

/// Parameters for a piledown coverage run, exposed to R.
#[extendr]
pub struct PileParams {
    input_bam: PathBuf,
    region: Option<String>,
    regions: Option<Vec<String>>,
    regions_file: Option<PathBuf>,
    strand: Strand,
    lib_fragment_type: LibFragmentType,
    exclude_flags: Option<u16>,
    index_path: Option<PathBuf>,
    concurrency: usize,
    chunk_size: Option<usize>,
}

impl PileParams {
    fn build_regions(&self) -> Result<Vec<PileRegion>> {
        if let Some(region_str) = &self.region {
            let pr = PileRegion::from_region_str(region_str, "region".into(), self.strand)?;
            Ok(vec![pr])
        } else if let Some(regions) = &self.regions {
            regions
                .iter()
                .enumerate()
                .map(|(i, r)| PileRegion::from_region_str(r, format!("region_{i}"), self.strand))
                .collect::<Result<Vec<_>>>()
        } else if let Some(path) = &self.regions_file {
            let file = std::fs::File::open(path)?;
            piledown::region::read_regions_tsv(file)
        } else {
            anyhow::bail!("no region source configured")
        }
    }

    fn run_engine(&self) -> Result<Vec<arrow::array::RecordBatch>> {
        use futures::StreamExt;

        let pile_regions = self.build_regions()?;

        let config = EngineConfig {
            bam_path: self.input_bam.clone(),
            exclude_flags: self.exclude_flags.map(Flags::from),
            lib_type: self.lib_fragment_type,
            concurrency: self.concurrency,
            index_path: self.index_path.clone(),
            chunk_size: self.chunk_size,
        };

        let engine = PileEngine::new(config);
        let rt = runtime();

        let results: Vec<(PileRegion, CoverageMap)> = rt.block_on(async {
            let stream = engine.run(pile_regions);
            let pinned = std::pin::pin!(stream);
            pinned
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
        })?;

        results
            .into_iter()
            .map(|(r, m)| to_record_batch(r, m))
            .collect::<Result<Vec<_>, _>>()
    }
}

#[extendr]
impl PileParams {
    /// Create a new PileParams for coverage computation.
    ///
    /// @param input_bam Path to indexed BAM file.
    /// @param strand One of "forward", "reverse", "either".
    /// @param lib_fragment_type One of "isr", "isf".
    /// @param region Optional region string (e.g. "chr1:100-200").
    /// @param regions Optional character vector of region strings.
    /// @param regions_file Optional path to TSV regions file.
    /// @param exclude_flags Optional SAM flags to exclude (integer 0-65535).
    /// @param index_path Optional explicit path to BAM index (.bai).
    /// @param concurrency Number of concurrent region processors (default 4).
    /// @param chunk_size Optional chunk size for splitting large regions.
    /// @export
    #[allow(clippy::too_many_arguments)]
    fn new(
        input_bam: &str,
        strand: &str,
        lib_fragment_type: &str,
        region: Option<&str>,
        regions: Option<Vec<String>>,
        regions_file: Option<&str>,
        exclude_flags: Option<i32>,
        index_path: Option<&str>,
        concurrency: Option<i32>,
        chunk_size: Option<i32>,
    ) -> Self {
        let strand = parse_strand(strand).unwrap_or_else(|e| panic!("{e}"));
        let lib_fragment_type = parse_lib_type(lib_fragment_type).unwrap_or_else(|e| panic!("{e}"));

        // Validate exactly one region source
        let sources = [region.is_some(), regions.is_some(), regions_file.is_some()];
        let count = sources.iter().filter(|&&s| s).count();
        if count != 1 {
            panic!("provide exactly one of: region, regions, regions_file");
        }

        // Validate and convert integer params
        let concurrency_val = concurrency.unwrap_or(4);
        if concurrency_val < 1 {
            panic!("concurrency must be >= 1");
        }
        let exclude_flags_val = match exclude_flags {
            Some(f) if !(0..=65535).contains(&f) => {
                panic!("exclude_flags must be between 0 and 65535")
            }
            Some(f) => Some(f as u16),
            None => None,
        };
        let chunk_size_val = match chunk_size {
            Some(c) if c < 1 => panic!("chunk_size must be >= 1"),
            Some(c) => Some(c as usize),
            None => None,
        };

        PileParams {
            input_bam: PathBuf::from(input_bam),
            region: region.map(String::from),
            regions,
            regions_file: regions_file.map(PathBuf::from),
            strand,
            lib_fragment_type,
            exclude_flags: exclude_flags_val,
            index_path: index_path.map(PathBuf::from),
            concurrency: concurrency_val as usize,
            chunk_size: chunk_size_val,
        }
    }

    /// Generate per-base coverage for the configured region(s).
    ///
    /// Returns a nanoarrow_array_stream that can be imported as an
    /// arrow::RecordBatchReader or converted to a data.frame.
    /// @export
    fn generate_stream(&self) -> extendr_api::Result<Robj> {
        let batches = self.run_engine().map_err(|e| Error::Other(e.to_string()))?;
        export_batches_to_r(batches)
    }
}

extendr_module! {
    mod piledownR;
    impl PileParams;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_no_region_source() {
        let params = PileParams {
            input_bam: PathBuf::from("test.bam"),
            region: None,
            regions: None,
            regions_file: None,
            strand: Strand::Forward,
            lib_fragment_type: LibFragmentType::Isr,
            exclude_flags: None,
            index_path: None,
            concurrency: 4,
            chunk_size: None,
        };
        assert!(params.build_regions().is_err());
    }

    #[test]
    fn builds_single_region() {
        let params = PileParams {
            input_bam: PathBuf::from("test.bam"),
            region: Some("chr1:100-200".into()),
            regions: None,
            regions_file: None,
            strand: Strand::Reverse,
            lib_fragment_type: LibFragmentType::Isr,
            exclude_flags: None,
            index_path: None,
            concurrency: 4,
            chunk_size: None,
        };
        let regions = params.build_regions().unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].seq, "chr1");
        assert_eq!(regions[0].strand, Strand::Reverse);
    }

    #[test]
    fn builds_multiple_regions() {
        let params = PileParams {
            input_bam: PathBuf::from("test.bam"),
            region: None,
            regions: Some(vec!["chr1:100-200".into(), "chr2:300-400".into()]),
            regions_file: None,
            strand: Strand::Either,
            lib_fragment_type: LibFragmentType::Isf,
            exclude_flags: None,
            index_path: None,
            concurrency: 4,
            chunk_size: None,
        };
        let regions = params.build_regions().unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[1].seq, "chr2");
    }

    #[test]
    fn parse_strand_accepts_variants() {
        assert_eq!(parse_strand("forward").unwrap(), Strand::Forward);
        assert_eq!(parse_strand("Forward").unwrap(), Strand::Forward);
        assert_eq!(parse_strand("FORWARD").unwrap(), Strand::Forward);
        assert_eq!(parse_strand("+").unwrap(), Strand::Forward);
        assert_eq!(parse_strand("reverse").unwrap(), Strand::Reverse);
        assert_eq!(parse_strand("-").unwrap(), Strand::Reverse);
        assert_eq!(parse_strand("either").unwrap(), Strand::Either);
        assert_eq!(parse_strand(".").unwrap(), Strand::Either);
        assert!(parse_strand("invalid").is_err());
    }

    #[test]
    fn parse_lib_type_accepts_variants() {
        assert_eq!(parse_lib_type("isr").unwrap(), LibFragmentType::Isr);
        assert_eq!(parse_lib_type("ISR").unwrap(), LibFragmentType::Isr);
        assert_eq!(parse_lib_type("isf").unwrap(), LibFragmentType::Isf);
        assert!(parse_lib_type("invalid").is_err());
    }
}
