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

    #[derive(Debug, Clone)]
    #[pyclass(str)]
    pub struct PileParams {
        #[pyo3(get)]
        pub input_bam: std::path::PathBuf,
        #[pyo3(get)]
        pub region: Option<String>,
        #[pyo3(get)]
        pub regions: Option<Vec<String>>,
        #[pyo3(get)]
        pub regions_file: Option<std::path::PathBuf>,
        #[pyo3(get)]
        pub strand: Strand,
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
    }

    #[pymethods]
    impl PileParams {
        #[new]
        #[pyo3(signature = (
            input_bam,
            strand,
            lib_fragment_type,
            region=None,
            regions=None,
            regions_file=None,
            exclude_flags=None,
            index_path=None,
            concurrency=4,
            chunk_size=None,
        ))]
        #[allow(clippy::too_many_arguments)]
        fn new(
            input_bam: std::path::PathBuf,
            strand: Strand,
            lib_fragment_type: LibFragmentType,
            region: Option<String>,
            regions: Option<Vec<String>>,
            regions_file: Option<std::path::PathBuf>,
            exclude_flags: Option<u16>,
            index_path: Option<std::path::PathBuf>,
            concurrency: usize,
            chunk_size: Option<usize>,
        ) -> PyResult<Self> {
            let sources = [region.is_some(), regions.is_some(), regions_file.is_some()];
            let count = sources.iter().filter(|&&s| s).count();
            if count != 1 {
                return Err(PyValueError::new_err(
                    "provide exactly one of: region, regions, regions_file",
                ));
            }
            Ok(Self {
                input_bam,
                region,
                regions,
                regions_file,
                strand,
                lib_fragment_type,
                exclude_flags,
                index_path,
                concurrency,
                chunk_size,
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
            py: Python<'_>,
        ) -> PyResult<PyArrowType<Box<dyn RecordBatchReader + Send>>> {
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

            let results: Vec<(PileRegion, CoverageMap)> = py
                .allow_threads(|| {
                    rt.block_on(async {
                        let stream = engine.run(pile_regions);
                        let pinned = std::pin::pin!(stream);
                        pinned
                            .collect::<Vec<_>>()
                            .await
                            .into_iter()
                            .collect::<Result<Vec<_>, _>>()
                    })
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
            let reader = arrow::record_batch::RecordBatchIterator::new(
                batches.into_iter().map(Ok),
                schema,
            );

            Ok(PyArrowType(Box::new(reader)))
        }
    }

    impl PileParams {
        fn build_regions(&self) -> PyResult<Vec<PileRegion>> {
            if let Some(region_str) = &self.region {
                let pr = PileRegion::from_region_str(region_str, "region".into(), self.strand)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?;
                Ok(vec![pr])
            } else if let Some(regions) = &self.regions {
                regions
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        PileRegion::from_region_str(r, format!("region_{i}"), self.strand)
                            .map_err(|e| PyValueError::new_err(e.to_string()))
                    })
                    .collect()
            } else if let Some(path) = &self.regions_file {
                let file = std::fs::File::open(path)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?;
                piledown::region::read_regions_tsv(file)
                    .map_err(|e| PyValueError::new_err(e.to_string()))
            } else {
                Err(PyValueError::new_err("no region source configured"))
            }
        }
    }

    impl Display for PileParams {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "PileParams(bam={:?}, lib_type={:?}, strand={:?}, concurrency={})",
                self.input_bam, self.lib_fragment_type, self.strand, self.concurrency
            )
        }
    }
}
