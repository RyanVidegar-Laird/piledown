pub mod structs;

use anyhow::{anyhow, Result};
use noodles::sam::alignment::record::Flags;
use structs::{LibFragmentType, Strand};
#[macro_use]
extern crate lazy_static;

pub fn get_strand(lib: LibFragmentType, flags: Flags) -> Result<Strand> {
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

#[pyo3::pymodule]
mod piledown {
    use pyo3::prelude::*;
    /// Formats the sum of two numbers as string.
    #[pyfunction]
    fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
        Ok((a + b).to_string())
    }
}
