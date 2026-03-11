use noodles::sam::alignment::record::Flags;

/// Composable record filter. Returns true if the record should be kept.
/// Send + Sync required for use across async tasks in the engine.
pub trait RecordFilter: Send + Sync {
    /// Check whether a record should be kept based on its flags.
    fn keep_flags(&self, flags: Flags) -> bool;
}

/// Exclude records matching any of the given flags.
pub struct FlagFilter(pub Flags);

impl RecordFilter for FlagFilter {
    fn keep_flags(&self, flags: Flags) -> bool {
        !flags.intersects(self.0)
    }
}

/// Apply a chain of filters — record must pass all of them.
pub fn apply_filters(flags: Flags, filters: &[Box<dyn RecordFilter>]) -> bool {
    filters.iter().all(|f| f.keep_flags(flags))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_filter_excludes_matching() {
        let filter = FlagFilter(Flags::UNMAPPED);
        let flags = Flags::from(0x4_u16); // UNMAPPED
        assert!(!filter.keep_flags(flags));
    }

    #[test]
    fn flag_filter_keeps_non_matching() {
        let filter = FlagFilter(Flags::UNMAPPED);
        let flags = Flags::from(0x3_u16); // PAIRED + PROPER_PAIR
        assert!(filter.keep_flags(flags));
    }

    #[test]
    fn flag_filter_excludes_when_any_bit_matches() {
        let filter = FlagFilter(Flags::UNMAPPED | Flags::DUPLICATE);
        let flags = Flags::from(0x4_u16); // only UNMAPPED set
        assert!(!filter.keep_flags(flags));
    }

    #[test]
    fn empty_filter_chain_keeps_all() {
        let filters: Vec<Box<dyn RecordFilter>> = vec![];
        let flags = Flags::from(0xFFFF_u16);
        assert!(apply_filters(flags, &filters));
    }

    #[test]
    fn filter_chain_all_must_pass() {
        let filters: Vec<Box<dyn RecordFilter>> = vec![
            Box::new(FlagFilter(Flags::UNMAPPED)),
            Box::new(FlagFilter(Flags::DUPLICATE)),
        ];
        // PAIRED + PROPER_PAIR — passes both
        assert!(apply_filters(Flags::from(0x3_u16), &filters));
        // UNMAPPED — fails first filter
        assert!(!apply_filters(Flags::from(0x4_u16), &filters));
        // DUPLICATE — fails second filter
        assert!(!apply_filters(Flags::from(0x400_u16), &filters));
    }
}
