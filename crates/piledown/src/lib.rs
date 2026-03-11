pub mod types;
pub mod coverage;
pub mod cigar;
pub mod strand;
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
                panic!("Unexpected flag sets: {flags:?}");
            }
        }
        LibFragmentType::Isf => todo!(),
    }
}
