#[pyo3::pymodule]
/// Rust bindings for `piledown` -- a simple utility to get coverage of matched *and* skipped bases from RNASeq BAMs.
mod pyledown {
    use std::fmt::Display;
    use std::fmt::Formatter;

    use arrow::array::RecordBatch;
    use arrow::pyarrow::PyArrowType;
    #[pymodule_export]
    use piledown::types::LibFragmentType;
    #[pymodule_export]
    use piledown::types::Strand;
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;
    use pyo3::types::PyString;

    use noodles::sam::alignment::record::Flags;
    use piledown::engine::{runtime, EngineConfig, PileEngine};
    use piledown::output::to_record_batch;
    use piledown::region::PileRegion;

    #[derive(Debug, Clone)]
    #[pyclass(str)]
    pub struct PileParams {
        #[pyo3(get)]
        pub input_bam: std::path::PathBuf,
        #[pyo3(get)]
        pub region: String,
        #[pyo3(get)]
        pub strand: Strand,
        #[pyo3(get)]
        pub lib_fragment_type: LibFragmentType,
        #[pyo3(get)]
        pub exclude_flags: Option<u16>,
    }

    #[pymethods]
    impl PileParams {
        #[new]
        #[pyo3(signature = (input_bam, region, strand, lib_fragment_type, exclude_flags=None))]
        fn new(
            input_bam: std::path::PathBuf,
            region: String,
            strand: Strand,
            lib_fragment_type: LibFragmentType,
            exclude_flags: Option<u16>,
        ) -> PyResult<Self> {
            Ok(Self {
                input_bam,
                region,
                strand,
                lib_fragment_type,
                exclude_flags,
            })
        }
        fn __repr__(slf: &Bound<'_, Self>) -> PyResult<String> {
            let class_name: Bound<'_, PyString> = slf.get_type().qualname()?;
            Ok(format!(
                "{}({:#?}, {:#?}, {:#?}, {:#?}, {:#?})",
                class_name,
                slf.borrow().input_bam,
                slf.borrow().region,
                slf.borrow().strand,
                slf.borrow().lib_fragment_type,
                slf.borrow().exclude_flags
            ))
        }

        fn generate(&self) -> PyResult<PyArrowType<RecordBatch>> {
            let pile_region =
                PileRegion::from_region_str(&self.region, "region".into(), self.strand)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?;

            let config = EngineConfig {
                bam_path: self.input_bam.clone(),
                exclude_flags: self.exclude_flags.map(Flags::from),
                lib_type: self.lib_fragment_type,
                concurrency: 1,
            };

            let engine = PileEngine::new(config);
            let rt = runtime();
            let results = rt
                .block_on(engine.run_collect(vec![pile_region]))
                .map_err(|e| PyValueError::new_err(e.to_string()))?;

            let (region, map) = results
                .into_iter()
                .next()
                .ok_or_else(|| PyValueError::new_err("no results"))?;
            let batch =
                to_record_batch(&region, &map).map_err(|e| PyValueError::new_err(e.to_string()))?;

            Ok(PyArrowType(batch))
        }
    }

    impl Display for PileParams {
        fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "PileParams: \
                  Input bam:{:#?} \
                  Query region:{:#?} \
                  Strand:{:#?} \
                  Library fragment type:{:#?} \
                  Exclude flags:{:#?}",
                self.input_bam,
                self.region,
                self.strand,
                self.lib_fragment_type,
                self.exclude_flags
            )
        }
    }
}
