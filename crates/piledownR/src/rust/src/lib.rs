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
    // Path 1: single region
    region: Option<String>,
    name: Option<String>,
    strand: Option<Strand>,
    // Path 2: region strings
    regions: Option<Vec<String>>,
    // Path 3: decomposed vectors (also used for DataFrame path via R wrapper)
    seqs: Option<Vec<String>>,
    starts: Option<Vec<f64>>,
    ends: Option<Vec<f64>>,
    // Shared by paths 2 and 3
    region_names: Option<Vec<String>>,
    region_strands: Option<Vec<String>>,
    // Path 5: TSV file
    regions_file: Option<PathBuf>,
    // Engine config
    lib_fragment_type: LibFragmentType,
    exclude_flags: Option<u16>,
    index_path: Option<PathBuf>,
    concurrency: usize,
    chunk_size: Option<usize>,
    anchor_length: u64,
}

impl PileParams {
    fn build_regions(&self) -> Result<Vec<PileRegion>> {
        if let Some(region_str) = &self.region {
            // Path 1: single region
            let name = self.name.clone().unwrap_or_else(|| "region".into());
            let strand = self.strand.unwrap_or(Strand::Either);
            let pr = PileRegion::from_region_str(region_str, name, strand)?;
            Ok(vec![pr])
        } else if let Some(regions) = &self.regions {
            // Path 2: region strings
            let names = self
                .region_names
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("'regions' requires 'region_names'"))?;
            let strands = self
                .region_strands
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("'regions' requires 'region_strands'"))?;
            if regions.len() != names.len() || regions.len() != strands.len() {
                anyhow::bail!(
                    "'regions' ({}), 'region_names' ({}), 'region_strands' ({}) must all be the same length",
                    regions.len(),
                    names.len(),
                    strands.len()
                );
            }
            regions
                .iter()
                .zip(names.iter())
                .zip(strands.iter())
                .map(|((r, n), s)| {
                    let strand = parse_strand(s)?;
                    PileRegion::from_region_str(r, n.clone(), strand)
                })
                .collect::<Result<Vec<_>>>()
        } else if let Some(seqs) = &self.seqs {
            // Path 3: decomposed vectors
            let starts = self
                .starts
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("'seqs' requires 'starts'"))?;
            let ends = self
                .ends
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("'seqs' requires 'ends'"))?;
            let names = self
                .region_names
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("'seqs' requires 'region_names'"))?;
            let strands = self
                .region_strands
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("'seqs' requires 'region_strands'"))?;
            let len = seqs.len();
            if starts.len() != len
                || ends.len() != len
                || names.len() != len
                || strands.len() != len
            {
                anyhow::bail!(
                    "'seqs' ({}), 'starts' ({}), 'ends' ({}), 'region_names' ({}), 'region_strands' ({}) must all be the same length",
                    len,
                    starts.len(),
                    ends.len(),
                    names.len(),
                    strands.len()
                );
            }
            seqs.iter()
                .zip(starts.iter())
                .zip(ends.iter())
                .zip(names.iter())
                .zip(strands.iter())
                .map(|((((seq, &start), &end), name), strand_str)| {
                    if !start.is_finite() || start < 0.0 {
                        anyhow::bail!("start must be a non-negative finite number, got {start}");
                    }
                    if !end.is_finite() || end < 0.0 {
                        anyhow::bail!("end must be a non-negative finite number, got {end}");
                    }
                    if start.fract() != 0.0 {
                        anyhow::bail!("start must be a whole number, got {start}");
                    }
                    if end.fract() != 0.0 {
                        anyhow::bail!("end must be a whole number, got {end}");
                    }
                    let strand = parse_strand(strand_str)?;
                    PileRegion::new(seq.clone(), start as u64, end as u64, name.clone(), strand)
                })
                .collect::<Result<Vec<_>>>()
        } else if let Some(path) = &self.regions_file {
            // Path 5: TSV file
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
            anchor_length: self.anchor_length,
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
    /// @param lib_fragment_type One of "isr", "isf".
    /// @param region Optional single region string (e.g. "chr1:100-200").
    /// @param name Region name (required with region).
    /// @param strand Strand string (required with region).
    /// @param regions Optional character vector of region strings.
    /// @param region_names Character vector of region names.
    /// @param region_strands Character vector of strand strings.
    /// @param seqs Optional character vector of sequence names.
    /// @param starts Numeric vector of start positions.
    /// @param ends Numeric vector of end positions.
    /// @param regions_file Optional path to TSV regions file.
    /// @param exclude_flags Optional SAM flags to exclude (integer 0-65535).
    /// @param index_path Optional explicit path to BAM index (.bai).
    /// @param concurrency Number of concurrent region processors (default 4).
    /// @param chunk_size Optional chunk size for splitting large regions.
    /// @param anchor_length Minimum matched bases flanking a junction (default NULL = 0, no filtering).
    /// @export
    #[allow(clippy::too_many_arguments)]
    fn new(
        input_bam: &str,
        lib_fragment_type: &str,
        region: Option<&str>,
        name: Option<&str>,
        strand: Option<&str>,
        regions: Option<Vec<String>>,
        region_names: Option<Vec<String>>,
        region_strands: Option<Vec<String>>,
        seqs: Option<Vec<String>>,
        starts: Option<Vec<f64>>,
        ends: Option<Vec<f64>>,
        regions_file: Option<&str>,
        exclude_flags: Option<i32>,
        index_path: Option<&str>,
        concurrency: Option<i32>,
        chunk_size: Option<i32>,
        anchor_length: Option<i32>,
    ) -> Self {
        let lib_fragment_type = parse_lib_type(lib_fragment_type).unwrap_or_else(|e| panic!("{e}"));
        let parsed_strand = strand.map(|s| parse_strand(s).unwrap_or_else(|e| panic!("{e}")));

        // Validate exactly one region source
        let sources = [
            region.is_some(),
            regions.is_some(),
            seqs.is_some(),
            regions_file.is_some(),
        ];
        let count = sources.iter().filter(|&&s| s).count();
        if count != 1 {
            panic!("provide exactly one of: region, regions, seqs, regions_file");
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
        let anchor_length_val = match anchor_length {
            Some(a) if a < 0 => panic!("anchor_length must be >= 0"),
            Some(a) => a as u64,
            None => 0,
        };

        PileParams {
            input_bam: PathBuf::from(input_bam),
            region: region.map(String::from),
            name: name.map(String::from),
            strand: parsed_strand,
            regions,
            seqs,
            starts,
            ends,
            region_names,
            region_strands,
            regions_file: regions_file.map(PathBuf::from),
            lib_fragment_type,
            exclude_flags: exclude_flags_val,
            index_path: index_path.map(PathBuf::from),
            concurrency: concurrency_val as usize,
            chunk_size: chunk_size_val,
            anchor_length: anchor_length_val,
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

    fn make_params_single() -> PileParams {
        PileParams {
            input_bam: PathBuf::from("test.bam"),
            region: Some("chr1:100-200".into()),
            name: Some("gene1".into()),
            strand: Some(Strand::Forward),
            regions: None,
            seqs: None,
            starts: None,
            ends: None,
            region_names: None,
            region_strands: None,
            regions_file: None,
            lib_fragment_type: LibFragmentType::Isr,
            exclude_flags: None,
            index_path: None,
            concurrency: 4,
            chunk_size: None,
            anchor_length: 0,
        }
    }

    #[test]
    fn builds_single_region() {
        let params = make_params_single();
        let regions = params.build_regions().unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].seq, "chr1");
        assert_eq!(regions[0].name, "gene1");
        assert_eq!(regions[0].strand, Strand::Forward);
    }

    #[test]
    fn builds_region_strings() {
        let params = PileParams {
            regions: Some(vec!["chr1:100-200".into(), "chr2:300-400".into()]),
            region_names: Some(vec!["g1".into(), "g2".into()]),
            region_strands: Some(vec!["+".into(), "-".into()]),
            region: None,
            name: None,
            strand: None,
            seqs: None,
            starts: None,
            ends: None,
            ..make_params_single()
        };
        let regions = params.build_regions().unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].strand, Strand::Forward);
        assert_eq!(regions[1].strand, Strand::Reverse);
    }

    #[test]
    fn builds_decomposed_vectors() {
        let params = PileParams {
            seqs: Some(vec!["chr1".into(), "chr2".into()]),
            starts: Some(vec![100.0, 300.0]),
            ends: Some(vec![200.0, 400.0]),
            region_names: Some(vec!["g1".into(), "g2".into()]),
            region_strands: Some(vec!["+".into(), ".".into()]),
            region: None,
            name: None,
            strand: None,
            regions: None,
            ..make_params_single()
        };
        let regions = params.build_regions().unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].start, 100);
        assert_eq!(regions[1].strand, Strand::Either);
    }

    #[test]
    fn rejects_no_region_source() {
        let params = PileParams {
            region: None,
            name: None,
            strand: None,
            regions: None,
            seqs: None,
            starts: None,
            ends: None,
            region_names: None,
            region_strands: None,
            regions_file: None,
            ..make_params_single()
        };
        assert!(params.build_regions().is_err());
    }

    #[test]
    fn rejects_length_mismatch() {
        let params = PileParams {
            regions: Some(vec!["chr1:100-200".into()]),
            region_names: Some(vec!["g1".into(), "g2".into()]),
            region_strands: Some(vec!["+".into()]),
            region: None,
            name: None,
            strand: None,
            seqs: None,
            starts: None,
            ends: None,
            ..make_params_single()
        };
        assert!(params.build_regions().is_err());
    }

    #[test]
    fn parse_strand_accepts_variants() {
        assert_eq!(parse_strand("forward").unwrap(), Strand::Forward);
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
        assert_eq!(parse_lib_type("isf").unwrap(), LibFragmentType::Isf);
        assert!(parse_lib_type("invalid").is_err());
    }

    #[test]
    fn rejects_fractional_start() {
        let params = PileParams {
            seqs: Some(vec!["chr1".into()]),
            starts: Some(vec![100.5]),
            ends: Some(vec![200.0]),
            region_names: Some(vec!["g1".into()]),
            region_strands: Some(vec!["+".into()]),
            region: None,
            name: None,
            strand: None,
            regions: None,
            regions_file: None,
            ..make_params_single()
        };
        assert!(params.build_regions().is_err());
    }

    #[test]
    fn rejects_fractional_end() {
        let params = PileParams {
            seqs: Some(vec!["chr1".into()]),
            starts: Some(vec![100.0]),
            ends: Some(vec![200.7]),
            region_names: Some(vec!["g1".into()]),
            region_strands: Some(vec!["+".into()]),
            region: None,
            name: None,
            strand: None,
            regions: None,
            regions_file: None,
            ..make_params_single()
        };
        assert!(params.build_regions().is_err());
    }
}
