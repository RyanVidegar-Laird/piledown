use anyhow::{anyhow, Result};
use noodles::sam::alignment::record::Flags;

use crate::types::Strand;

/// Classifies the original transcript strand from BAM flags based on library prep protocol.
/// Send + Sync required for use across async tasks in the engine.
pub trait StrandClassifier: Send + Sync {
    fn classify(&self, flags: Flags) -> Result<Strand>;
}

pub struct IsrClassifier;
pub struct IsfClassifier;

impl StrandClassifier for IsrClassifier {
    fn classify(&self, flags: Flags) -> Result<Strand> {
        if !flags.is_segmented() || !flags.is_properly_segmented() {
            return Err(anyhow!("not paired/proper: cannot determine strand"));
        }

        let is_reverse = flags.is_reverse_complemented();
        let is_mate_reverse = flags.is_mate_reverse_complemented();
        let is_first = flags.is_first_segment();
        let is_last = flags.is_last_segment();

        // ISR: "inward-stranded-reverse"
        // Read 1 reverse-complemented → Forward strand
        // Read 2 mate-reverse-complemented → Forward strand
        // Read 1 mate-reverse-complemented → Reverse strand
        // Read 2 reverse-complemented → Reverse strand
        if (is_first && is_reverse) || (is_last && is_mate_reverse) {
            Ok(Strand::Forward)
        } else if (is_first && is_mate_reverse) || (is_last && is_reverse) {
            Ok(Strand::Reverse)
        } else {
            Err(anyhow!("unexpected flag combination: {:?}", flags))
        }
    }
}

impl StrandClassifier for IsfClassifier {
    fn classify(&self, flags: Flags) -> Result<Strand> {
        if !flags.is_segmented() || !flags.is_properly_segmented() {
            return Err(anyhow!("not paired/proper: cannot determine strand"));
        }

        let is_reverse = flags.is_reverse_complemented();
        let is_mate_reverse = flags.is_mate_reverse_complemented();
        let is_first = flags.is_first_segment();
        let is_last = flags.is_last_segment();

        // ISF: "inward-stranded-forward" — mirror of ISR
        // Read 1 forward-mapped (not reverse) → Forward strand
        // Read 2 reverse-complemented → Forward strand
        // Read 1 reverse-complemented → Reverse strand
        // Read 2 forward-mapped (not reverse) → Reverse strand
        if (is_first && is_mate_reverse && !is_reverse) || (is_last && is_reverse) {
            Ok(Strand::Forward)
        } else if (is_first && is_reverse) || (is_last && is_mate_reverse && !is_reverse) {
            Ok(Strand::Reverse)
        } else {
            Err(anyhow!("unexpected flag combination: {:?}", flags))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flags(bits: u16) -> Flags {
        Flags::from(bits)
    }

    // ISR tests

    #[test]
    fn isr_read1_reverse_is_forward() {
        let c = IsrClassifier;
        // PAIRED | PROPER_PAIR | REVERSE | FIRST_SEGMENT
        let f = flags(0x1 | 0x2 | 0x10 | 0x40);
        assert_eq!(c.classify(f).unwrap(), Strand::Forward);
    }

    #[test]
    fn isr_read2_mate_reverse_is_forward() {
        let c = IsrClassifier;
        // PAIRED | PROPER_PAIR | MATE_REVERSE | LAST_SEGMENT
        let f = flags(0x1 | 0x2 | 0x20 | 0x80);
        assert_eq!(c.classify(f).unwrap(), Strand::Forward);
    }

    #[test]
    fn isr_read1_mate_reverse_is_reverse() {
        let c = IsrClassifier;
        // PAIRED | PROPER_PAIR | MATE_REVERSE | FIRST_SEGMENT
        let f = flags(0x1 | 0x2 | 0x20 | 0x40);
        assert_eq!(c.classify(f).unwrap(), Strand::Reverse);
    }

    #[test]
    fn isr_read2_reverse_is_reverse() {
        let c = IsrClassifier;
        // PAIRED | PROPER_PAIR | REVERSE | LAST_SEGMENT
        let f = flags(0x1 | 0x2 | 0x10 | 0x80);
        assert_eq!(c.classify(f).unwrap(), Strand::Reverse);
    }

    #[test]
    fn isr_unpaired_returns_error() {
        let c = IsrClassifier;
        let f = flags(0x0);
        assert!(c.classify(f).is_err());
    }

    // ISF tests

    #[test]
    fn isf_read1_forward_is_forward() {
        let c = IsfClassifier;
        // PAIRED | PROPER_PAIR | MATE_REVERSE | FIRST_SEGMENT
        // (read1 is NOT reverse-complemented, mate IS)
        let f = flags(0x1 | 0x2 | 0x20 | 0x40);
        assert_eq!(c.classify(f).unwrap(), Strand::Forward);
    }

    #[test]
    fn isf_read2_reverse_is_forward() {
        let c = IsfClassifier;
        // PAIRED | PROPER_PAIR | REVERSE | LAST_SEGMENT
        let f = flags(0x1 | 0x2 | 0x10 | 0x80);
        assert_eq!(c.classify(f).unwrap(), Strand::Forward);
    }

    #[test]
    fn isf_read1_reverse_is_reverse() {
        let c = IsfClassifier;
        // PAIRED | PROPER_PAIR | REVERSE | FIRST_SEGMENT
        let f = flags(0x1 | 0x2 | 0x10 | 0x40);
        assert_eq!(c.classify(f).unwrap(), Strand::Reverse);
    }

    #[test]
    fn isf_read2_forward_is_reverse() {
        let c = IsfClassifier;
        // PAIRED | PROPER_PAIR | MATE_REVERSE | LAST_SEGMENT
        // (read2 is NOT reverse-complemented, mate IS)
        let f = flags(0x1 | 0x2 | 0x20 | 0x80);
        assert_eq!(c.classify(f).unwrap(), Strand::Reverse);
    }
}
