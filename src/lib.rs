pub mod structs;

use anyhow::{anyhow, Result};
use noodles::sam::alignment::record::Flags;
use std::cell::LazyCell;
use structs::{LibFragmentType, Strand};

pub fn get_strand(lib: LibFragmentType, flags: Flags) -> Result<Strand> {
    if !flags.is_segmented() | !flags.is_properly_segmented() {
        return Err(anyhow!("not enough info to determine strand"));
    }

    // The below bitflags are known at compile time, but hardcoding them is less
    // reader friendly. Instead, use a LazyCell to only eval them once during runtime

    // forward read flags for ISR
    let isr_f1_flags = LazyCell::new(|| Flags::REVERSE_COMPLEMENTED | Flags::FIRST_SEGMENT);
    let isr_f2_flags = LazyCell::new(|| Flags::MATE_REVERSE_COMPLEMENTED | Flags::LAST_SEGMENT);

    // reverse read flags for ISR
    let isr_r1_flags = LazyCell::new(|| Flags::FIRST_SEGMENT | Flags::MATE_REVERSE_COMPLEMENTED);
    let isr_r2_flags = LazyCell::new(|| Flags::REVERSE_COMPLEMENTED | Flags::LAST_SEGMENT);

    match lib {
        LibFragmentType::Isr => {
            if flags.contains(*isr_f1_flags) | flags.contains(*isr_f2_flags) {
                Ok(Strand::Forward)
            } else if flags.contains(*isr_r1_flags) | flags.contains(*isr_r2_flags) {
                Ok(Strand::Reverse)
            } else {
                panic!("Unexpected flag sets: {:?}", flags);
            }
        }
        LibFragmentType::Isf => todo!(),
    }
}

#[pyo3::pymodule]
/// Rust bindings for `piledown` -- a simple utility to get coverage of matched *and* skipped bases from RNASeq BAMs.
mod piledown {
    use std::fmt::Display;
    use std::fmt::Formatter;

    #[pymodule_export]
    use crate::structs::LibFragmentType;
    use crate::structs::Pile;
    #[pymodule_export]
    use crate::structs::Strand;
    use arrow::array::RecordBatch;
    use arrow::pyarrow::PyArrowType;
    use pyo3::exceptions::PyValueError;
    use pyo3::prelude::*;
    use pyo3::types::PyString;

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
            // This is the equivalent of `self.__class__.__name__` in Python.
            let class_name: Bound<'_, PyString> = slf.get_type().qualname()?;
            // To access fields of the Rust struct, we need to borrow the `PyCell`.
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
            let mut pile =
                Pile::try_from(self).map_err(|e| PyValueError::new_err(e.to_string()))?;

            pile.generate()?;
            let batch = pile.to_record_batch()?;

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
