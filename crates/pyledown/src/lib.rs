#[pyo3::pymodule]
/// Rust bindings for `piledown` -- coverage of matched and skipped bases from RNASeq BAMs.
mod pyledown {
    use std::fmt::Display;
    use std::fmt::Formatter;

    use arrow::array::RecordBatch;
    use arrow::pyarrow::PyArrowType;
    use arrow::record_batch::RecordBatchReader;
    #[pymodule_export]
    use piledown::types::LibFragmentType;
    #[pymodule_export]
    use piledown::types::Strand;
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;
    use pyo3::types::PyString;

    use futures::StreamExt;
    use noodles::sam::alignment::record::Flags;
    use piledown::coverage::CoverageMap;
    use piledown::engine::{runtime, EngineConfig, PileEngine};
    use piledown::output::to_record_batch;
    use piledown::region::PileRegion;

    fn parse_strand_py(s: &str) -> PyResult<Strand> {
        match s.to_lowercase().as_str() {
            "forward" | "+" => Ok(Strand::Forward),
            "reverse" | "-" => Ok(Strand::Reverse),
            "either" | "." => Ok(Strand::Either),
            _ => Err(PyValueError::new_err(format!(
                "invalid strand: '{s}'. Expected: forward, reverse, either, +, -, ."
            ))),
        }
    }

    fn extract_dataframe(df: &Bound<'_, PyAny>) -> PyResult<Vec<PileRegion>> {
        let seq_col: Vec<String> = df.get_item("seq")?.extract()?;
        let start_col: Vec<u64> = df.get_item("start")?.extract()?;
        let end_col: Vec<u64> = df.get_item("end")?.extract()?;
        let name_col: Vec<String> = df.get_item("name")?.extract()?;
        let strand_col: Vec<String> = df.get_item("strand")?.extract()?;
        let anchor_col: Option<Vec<u64>> = df
            .get_item("anchor")
            .ok()
            .and_then(|col| col.extract().ok());

        let mut regions = Vec::with_capacity(seq_col.len());
        for i in 0..seq_col.len() {
            let strand = parse_strand_py(&strand_col[i])?;
            let mut pr = PileRegion::new(
                seq_col[i].clone(),
                start_col[i],
                end_col[i],
                name_col[i].clone(),
                strand,
            )
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
            if let Some(ref anchors) = anchor_col {
                pr.anchor_length = Some(anchors[i]);
            }
            regions.push(pr);
        }
        Ok(regions)
    }

    #[derive(Debug, Clone)]
    #[pyclass(str, from_py_object)]
    pub struct PileParams {
        #[pyo3(get)]
        pub input_bam: std::path::PathBuf,
        // Path 1: single region
        #[pyo3(get)]
        pub region: Option<String>,
        #[pyo3(get)]
        pub name: Option<String>,
        #[pyo3(get)]
        pub strand: Option<Strand>,
        // Path 2: region strings
        #[pyo3(get)]
        pub regions: Option<Vec<String>>,
        // Path 3: decomposed vectors
        #[pyo3(get)]
        pub seqs: Option<Vec<String>>,
        #[pyo3(get)]
        pub starts: Option<Vec<u64>>,
        #[pyo3(get)]
        pub ends: Option<Vec<u64>>,
        // Shared by paths 2 and 3
        #[pyo3(get)]
        pub names: Option<Vec<String>>,
        #[pyo3(get)]
        pub strands: Option<Vec<Strand>>,
        // Path 4: DataFrame (stored as pre-parsed regions)
        pub regions_df: Option<Vec<PileRegion>>,
        // Path 5: TSV file
        #[pyo3(get)]
        pub regions_file: Option<std::path::PathBuf>,
        // Engine config
        #[pyo3(get)]
        pub lib_fragment_type: LibFragmentType,
        #[pyo3(get)]
        pub exclude_flags: Option<u16>,
        #[pyo3(get)]
        pub index_path: Option<std::path::PathBuf>,
        #[pyo3(get)]
        pub concurrency: usize,
        #[pyo3(get)]
        pub chunk_size: Option<usize>,
        #[pyo3(get)]
        pub anchor_length: u64,
        #[pyo3(get)]
        pub anchor_lengths: Option<Vec<u64>>,
    }

    #[pymethods]
    impl PileParams {
        #[new]
        #[pyo3(signature = (
            input_bam,
            lib_fragment_type,
            region=None,
            name=None,
            strand=None,
            regions=None,
            names=None,
            strands=None,
            seqs=None,
            starts=None,
            ends=None,
            regions_df=None,
            regions_file=None,
            exclude_flags=None,
            index_path=None,
            concurrency=4,
            chunk_size=None,
            anchor_length=0,
            anchor_lengths=None,
        ))]
        #[allow(clippy::too_many_arguments)]
        fn new(
            input_bam: std::path::PathBuf,
            lib_fragment_type: LibFragmentType,
            region: Option<String>,
            name: Option<String>,
            strand: Option<Strand>,
            regions: Option<Vec<String>>,
            names: Option<Vec<String>>,
            strands: Option<Vec<Strand>>,
            seqs: Option<Vec<String>>,
            starts: Option<Vec<u64>>,
            ends: Option<Vec<u64>>,
            regions_df: Option<&Bound<'_, PyAny>>,
            regions_file: Option<std::path::PathBuf>,
            exclude_flags: Option<u16>,
            index_path: Option<std::path::PathBuf>,
            concurrency: usize,
            chunk_size: Option<usize>,
            anchor_length: u64,
            anchor_lengths: Option<Vec<u64>>,
        ) -> PyResult<Self> {
            // Check exactly one group key is present
            let sources = [
                region.is_some(),
                regions.is_some(),
                seqs.is_some(),
                regions_df.is_some(),
                regions_file.is_some(),
            ];
            let count = sources.iter().filter(|&&s| s).count();
            if count != 1 {
                return Err(PyValueError::new_err(
                    "provide exactly one of: region, regions, seqs, regions_df, regions_file",
                ));
            }

            if concurrency == 0 {
                return Err(PyValueError::new_err("concurrency must be >= 1"));
            }
            if let Some(cs) = chunk_size {
                if cs == 0 {
                    return Err(PyValueError::new_err("chunk_size must be >= 1"));
                }
            }

            // Validate companions for the active group
            if region.is_some() {
                // Path 1: reject companions from other groups
                if regions.is_some()
                    || seqs.is_some()
                    || starts.is_some()
                    || ends.is_some()
                    || names.is_some()
                    || strands.is_some()
                {
                    return Err(PyValueError::new_err(
                        "single region mode: use 'name' and 'strand', not vector parameters",
                    ));
                }
            } else if regions.is_some() {
                // Path 2: reject companions from other groups
                if name.is_some()
                    || strand.is_some()
                    || seqs.is_some()
                    || starts.is_some()
                    || ends.is_some()
                {
                    return Err(PyValueError::new_err(
                        "region strings mode: use 'names' and 'strands', not single or decomposed parameters",
                    ));
                }
            } else if seqs.is_some() {
                // Path 3: reject companions from other groups
                if name.is_some() || strand.is_some() || regions.is_some() {
                    return Err(PyValueError::new_err(
                        "decomposed mode: use 'starts', 'ends', 'names', 'strands', not single or region string parameters",
                    ));
                }
            } else if regions_df.is_some() || regions_file.is_some() {
                // Paths 4 and 5: reject all companions
                if name.is_some()
                    || strand.is_some()
                    || names.is_some()
                    || strands.is_some()
                    || regions.is_some()
                    || seqs.is_some()
                    || starts.is_some()
                    || ends.is_some()
                {
                    return Err(PyValueError::new_err(
                        "DataFrame/TSV mode: no other region parameters allowed",
                    ));
                }
            }

            // Extract DataFrame if provided
            let parsed_df = match regions_df {
                Some(df) => Some(extract_dataframe(df)?),
                None => None,
            };

            Ok(Self {
                input_bam,
                region,
                name,
                strand,
                regions,
                seqs,
                starts,
                ends,
                names,
                strands,
                regions_df: parsed_df,
                regions_file,
                lib_fragment_type,
                exclude_flags,
                index_path,
                concurrency,
                chunk_size,
                anchor_length,
                anchor_lengths,
            })
        }

        fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
            let class_name: Bound<'_, PyString> = slf.get_type().qualname()?;
            Ok(format!("{}({:#?})", class_name, slf.borrow().input_bam))
        }

        /// Generate per-base coverage for the configured region(s).
        ///
        /// Returns a PyArrow RecordBatchReader yielding batches with columns:
        /// name, seq, strand, pos, up, down.
        fn generate(
            &self,
            _py: Python<'_>,
        ) -> PyResult<PyArrowType<Box<dyn RecordBatchReader + Send>>> {
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

            let results: Vec<(PileRegion, CoverageMap)> = rt
                .block_on(async {
                    let stream = engine.run(pile_regions);
                    let pinned = std::pin::pin!(stream);
                    pinned
                        .collect::<Vec<_>>()
                        .await
                        .into_iter()
                        .collect::<Result<Vec<_>, _>>()
                })
                .map_err(|e| PyValueError::new_err(e.to_string()))?;

            let batches: Vec<RecordBatch> = results
                .into_iter()
                .map(|(r, m)| to_record_batch(r, m))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| PyValueError::new_err(e.to_string()))?;

            if batches.is_empty() {
                return Err(PyValueError::new_err("no regions produced output"));
            }

            let schema = batches[0].schema();
            let reader =
                arrow::record_batch::RecordBatchIterator::new(batches.into_iter().map(Ok), schema);

            Ok(PyArrowType(Box::new(reader)))
        }
    }

    impl PileParams {
        pub(crate) fn build_regions(&self) -> PyResult<Vec<PileRegion>> {
            let mut regions = if let Some(region_str) = &self.region {
                // Path 1: single region
                let name = self.name.as_deref().ok_or_else(|| {
                    PyValueError::new_err("'region' requires 'name' and 'strand'")
                })?;
                let strand = self.strand.ok_or_else(|| {
                    PyValueError::new_err("'region' requires 'name' and 'strand'")
                })?;
                let pr = PileRegion::from_region_str(region_str, name.to_string(), strand)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?;
                Ok(vec![pr])
            } else if let Some(regions) = &self.regions {
                // Path 2: region strings with names and strands
                let names = self.names.as_ref().ok_or_else(|| {
                    PyValueError::new_err(
                        "'regions' requires 'names' and 'strands' of equal length",
                    )
                })?;
                let strands = self.strands.as_ref().ok_or_else(|| {
                    PyValueError::new_err(
                        "'regions' requires 'names' and 'strands' of equal length",
                    )
                })?;
                if regions.len() != names.len() || regions.len() != strands.len() {
                    return Err(PyValueError::new_err(format!(
                        "'regions' ({}), 'names' ({}), 'strands' ({}) must all be the same length",
                        regions.len(),
                        names.len(),
                        strands.len()
                    )));
                }
                regions
                    .iter()
                    .zip(names)
                    .zip(strands)
                    .map(|((r, n), &s)| {
                        PileRegion::from_region_str(r, n.clone(), s)
                            .map_err(|e| PyValueError::new_err(e.to_string()))
                    })
                    .collect()
            } else if let Some(seqs) = &self.seqs {
                // Path 3: decomposed vectors
                let starts = self.starts.as_ref().ok_or_else(|| {
                    PyValueError::new_err(
                        "'seqs' requires 'starts', 'ends', 'names', and 'strands'",
                    )
                })?;
                let ends = self.ends.as_ref().ok_or_else(|| {
                    PyValueError::new_err(
                        "'seqs' requires 'starts', 'ends', 'names', and 'strands'",
                    )
                })?;
                let names = self.names.as_ref().ok_or_else(|| {
                    PyValueError::new_err(
                        "'seqs' requires 'starts', 'ends', 'names', and 'strands'",
                    )
                })?;
                let strands = self.strands.as_ref().ok_or_else(|| {
                    PyValueError::new_err(
                        "'seqs' requires 'starts', 'ends', 'names', and 'strands'",
                    )
                })?;
                let len = seqs.len();
                if starts.len() != len
                    || ends.len() != len
                    || names.len() != len
                    || strands.len() != len
                {
                    return Err(PyValueError::new_err(format!(
                        "'seqs' ({}), 'starts' ({}), 'ends' ({}), 'names' ({}), 'strands' ({}) must all be the same length",
                        len, starts.len(), ends.len(), names.len(), strands.len()
                    )));
                }
                seqs.iter()
                    .zip(starts)
                    .zip(ends)
                    .zip(names)
                    .zip(strands)
                    .map(|((((seq, &start), &end), name), &strand)| {
                        PileRegion::new(seq.clone(), start, end, name.clone(), strand)
                            .map_err(|e| PyValueError::new_err(e.to_string()))
                    })
                    .collect()
            } else if let Some(df_regions) = &self.regions_df {
                // Path 4: pre-parsed DataFrame
                Ok(df_regions.clone())
            } else if let Some(path) = &self.regions_file {
                // Path 5: TSV file
                let file =
                    std::fs::File::open(path).map_err(|e| PyValueError::new_err(e.to_string()))?;
                piledown::region::read_regions_tsv(file)
                    .map_err(|e| PyValueError::new_err(e.to_string()))
            } else {
                return Err(PyValueError::new_err("no region source configured"));
            }?;

            // Apply per-region anchor lengths if provided
            if let Some(anchors) = &self.anchor_lengths {
                if anchors.len() != regions.len() {
                    return Err(PyValueError::new_err(format!(
                        "'anchor_lengths' ({}) must match region count ({})",
                        anchors.len(),
                        regions.len()
                    )));
                }
                for (region, &anchor) in regions.iter_mut().zip(anchors.iter()) {
                    region.anchor_length = Some(anchor);
                }
            }
            Ok(regions)
        }
    }

    impl Display for PileParams {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "PileParams(bam={:?}, lib_type={:?}, concurrency={})",
                self.input_bam, self.lib_fragment_type, self.concurrency
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pyledown::*;
    use piledown::types::{LibFragmentType, Strand};
    use std::io::Write;

    /// Helper to build a PileParams directly (bypassing PyO3 constructor).
    fn base_params() -> PileParams {
        PileParams {
            input_bam: "test.bam".into(),
            region: None,
            name: None,
            strand: None,
            regions: None,
            seqs: None,
            starts: None,
            ends: None,
            names: None,
            strands: None,
            regions_df: None,
            regions_file: None,
            lib_fragment_type: LibFragmentType::Isr,
            exclude_flags: None,
            index_path: None,
            concurrency: 4,
            chunk_size: None,
            anchor_length: 0,
            anchor_lengths: None,
        }
    }

    #[test]
    fn build_regions_single() {
        let mut params = base_params();
        params.region = Some("chr1:1000-2000".to_string());
        params.name = Some("my_region".to_string());
        params.strand = Some(Strand::Forward);

        let regions = params.build_regions().unwrap();
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].seq, "chr1");
        assert_eq!(regions[0].start, 1000);
        assert_eq!(regions[0].end, 2000);
        assert_eq!(regions[0].name, "my_region");
        assert_eq!(regions[0].strand, Strand::Forward);
    }

    #[test]
    fn build_regions_parallel_region_strings() {
        let mut params = base_params();
        params.regions = Some(vec![
            "chr1:1000-2000".to_string(),
            "chr2:3000-4000".to_string(),
        ]);
        params.names = Some(vec!["r1".to_string(), "r2".to_string()]);
        params.strands = Some(vec![Strand::Forward, Strand::Reverse]);

        let regions = params.build_regions().unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].name, "r1");
        assert_eq!(regions[0].strand, Strand::Forward);
        assert_eq!(regions[1].seq, "chr2");
        assert_eq!(regions[1].name, "r2");
        assert_eq!(regions[1].strand, Strand::Reverse);
    }

    #[test]
    fn build_regions_decomposed_vectors() {
        let mut params = base_params();
        params.seqs = Some(vec!["chr1".to_string(), "chr2".to_string()]);
        params.starts = Some(vec![100, 200]);
        params.ends = Some(vec![500, 600]);
        params.names = Some(vec!["a".to_string(), "b".to_string()]);
        params.strands = Some(vec![Strand::Either, Strand::Reverse]);

        let regions = params.build_regions().unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].seq, "chr1");
        assert_eq!(regions[0].start, 100);
        assert_eq!(regions[0].end, 500);
        assert_eq!(regions[0].name, "a");
        assert_eq!(regions[0].strand, Strand::Either);
        assert_eq!(regions[1].seq, "chr2");
        assert_eq!(regions[1].start, 200);
        assert_eq!(regions[1].end, 600);
        assert_eq!(regions[1].name, "b");
        assert_eq!(regions[1].strand, Strand::Reverse);
    }

    #[test]
    fn build_regions_tsv_file() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "seq\tstart\tend\tname\tstrand").unwrap();
        writeln!(tmp, "chr1\t1000\t2000\tgene1\t+").unwrap();
        writeln!(tmp, "chr2\t3000\t4000\tgene2\t-").unwrap();
        tmp.flush().unwrap();

        let mut params = base_params();
        params.regions_file = Some(tmp.path().to_path_buf());

        let regions = params.build_regions().unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].name, "gene1");
        assert_eq!(regions[0].strand, Strand::Forward);
        assert_eq!(regions[1].name, "gene2");
        assert_eq!(regions[1].strand, Strand::Reverse);
    }

    #[test]
    fn build_regions_rejects_no_source() {
        let params = base_params();
        assert!(params.build_regions().is_err());
    }

    #[test]
    fn build_regions_rejects_length_mismatch() {
        let mut params = base_params();
        params.regions = Some(vec![
            "chr1:1000-2000".to_string(),
            "chr2:3000-4000".to_string(),
        ]);
        params.names = Some(vec!["r1".to_string()]); // only 1 name for 2 regions
        params.strands = Some(vec![Strand::Forward, Strand::Reverse]);

        assert!(params.build_regions().is_err());
    }
}
