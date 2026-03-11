use crate::cigar::CigarSpan;

/// Per-base coverage counts.
#[derive(Clone, Debug, Default)]
pub struct Coverage {
    pub up: u64,
    pub down: u64,
}

/// Vec-backed coverage over a contiguous genomic region.
/// Positions are 1-based, inclusive at both ends (matching BAM/noodles conventions).
/// Indexed by `(pos - start)` for O(1) access.
#[derive(Clone, Debug)]
pub struct CoverageMap {
    pub start: u64,
    pub end: u64,
    pub counts: Vec<Coverage>,
}

impl CoverageMap {
    /// Create a new CoverageMap covering positions `start..=end`, all zeroed.
    pub fn new(start: u64, end: u64) -> Self {
        let len = (end - start + 1) as usize;
        Self {
            start,
            end,
            counts: vec![Coverage::default(); len],
        }
    }

    /// Number of positions tracked.
    pub fn len(&self) -> usize {
        self.counts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }

    /// Get coverage at a genomic position. Returns None if out of range.
    pub fn get(&self, pos: u64) -> Option<&Coverage> {
        if pos < self.start || pos > self.end {
            return None;
        }
        self.counts.get((pos - self.start) as usize)
    }

    /// Get mutable coverage at a genomic position. Returns None if out of range.
    pub fn get_mut(&mut self, pos: u64) -> Option<&mut Coverage> {
        if pos < self.start || pos > self.end {
            return None;
        }
        self.counts.get_mut((pos - self.start) as usize)
    }

    /// Apply CIGAR spans to update coverage counts.
    /// Spans outside the region bounds are safely clipped.
    pub fn apply_spans(&mut self, spans: &[CigarSpan]) {
        for span in spans {
            let (span_start, span_len, is_up) = match span {
                CigarSpan::Match { start, len } => (*start, *len, true),
                CigarSpan::Skip { start, len } => (*start, *len, false),
            };

            let span_end = span_start + span_len; // exclusive

            // Clip to region bounds
            let effective_start = span_start.max(self.start);
            let effective_end = span_end.min(self.end + 1);

            if effective_start >= effective_end {
                continue;
            }

            let idx_start = (effective_start - self.start) as usize;
            let idx_end = (effective_end - self.start) as usize;

            for cov in &mut self.counts[idx_start..idx_end] {
                if is_up {
                    cov.up += 1;
                } else {
                    cov.down += 1;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_zeroed() {
        let map = CoverageMap::new(100, 105);
        assert_eq!(map.len(), 6); // 100..=105
        for cov in map.counts.iter() {
            assert_eq!(cov.up, 0);
            assert_eq!(cov.down, 0);
        }
    }

    #[test]
    fn get_returns_correct_position() {
        let mut map = CoverageMap::new(100, 102);
        map.counts[1].up = 42;
        assert_eq!(map.get(101).unwrap().up, 42);
    }

    #[test]
    fn get_out_of_bounds_returns_none() {
        let map = CoverageMap::new(100, 102);
        assert!(map.get(99).is_none());
        assert!(map.get(103).is_none());
    }

    #[test]
    fn get_mut_modifies_in_place() {
        let mut map = CoverageMap::new(100, 102);
        if let Some(cov) = map.get_mut(101) {
            cov.up += 5;
        }
        assert_eq!(map.get(101).unwrap().up, 5);
    }

    #[test]
    fn single_position_region() {
        let map = CoverageMap::new(100, 100);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(100).unwrap().up, 0);
    }

    #[test]
    fn apply_match_span() {
        let mut map = CoverageMap::new(100, 109);
        let spans = vec![CigarSpan::Match { start: 102, len: 3 }];
        map.apply_spans(&spans);
        assert_eq!(map.get(101).unwrap().up, 0);
        assert_eq!(map.get(102).unwrap().up, 1);
        assert_eq!(map.get(103).unwrap().up, 1);
        assert_eq!(map.get(104).unwrap().up, 1);
        assert_eq!(map.get(105).unwrap().up, 0);
    }

    #[test]
    fn apply_skip_span() {
        let mut map = CoverageMap::new(100, 109);
        let spans = vec![CigarSpan::Skip { start: 103, len: 2 }];
        map.apply_spans(&spans);
        assert_eq!(map.get(103).unwrap().down, 1);
        assert_eq!(map.get(104).unwrap().down, 1);
        assert_eq!(map.get(105).unwrap().down, 0);
    }

    #[test]
    fn apply_spans_clips_to_region_bounds() {
        let mut map = CoverageMap::new(100, 104);
        let spans = vec![CigarSpan::Match { start: 103, len: 10 }];
        map.apply_spans(&spans);
        assert_eq!(map.get(103).unwrap().up, 1);
        assert_eq!(map.get(104).unwrap().up, 1);
    }

    #[test]
    fn apply_spans_before_region_ignored() {
        let mut map = CoverageMap::new(100, 104);
        let spans = vec![CigarSpan::Match { start: 95, len: 3 }];
        map.apply_spans(&spans);
        for cov in map.counts.iter() {
            assert_eq!(cov.up, 0);
        }
    }

    #[test]
    fn apply_spans_straddling_region_start() {
        let mut map = CoverageMap::new(100, 104);
        let spans = vec![CigarSpan::Match { start: 98, len: 5 }];
        map.apply_spans(&spans);
        assert_eq!(map.get(100).unwrap().up, 1);
        assert_eq!(map.get(101).unwrap().up, 1);
        assert_eq!(map.get(102).unwrap().up, 1);
    }
}
