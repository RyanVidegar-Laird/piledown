use crate::cigar::CigarSpan;

/// Vec-backed coverage over a contiguous genomic region.
/// Positions are 1-based, inclusive at both ends (matching BAM/noodles conventions).
/// Indexed by `(pos - start)` for O(1) access.
///
/// Uses struct-of-arrays layout: separate contiguous `Vec<u64>` for `up` and `down`
/// counts. This enables zero-copy handoff to Arrow arrays via `Buffer::from_vec()`,
/// SIMD-friendly auto-vectorization, and cache-friendly single-field iteration.
#[derive(Clone, Debug)]
pub struct CoverageMap {
    pub start: u64,
    pub end: u64,
    pub up: Vec<u64>,
    pub down: Vec<u64>,
}

impl CoverageMap {
    /// Create a new CoverageMap covering positions `start..=end`, all zeroed.
    pub fn new(start: u64, end: u64) -> Self {
        assert!(
            start <= end,
            "CoverageMap::new called with start ({start}) > end ({end})"
        );
        let len = (end - start + 1) as usize;
        Self {
            start,
            end,
            up: vec![0u64; len],
            down: vec![0u64; len],
        }
    }

    /// Number of positions tracked.
    pub fn len(&self) -> usize {
        self.up.len()
    }

    pub fn is_empty(&self) -> bool {
        self.up.is_empty()
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

            let arr = if is_up { &mut self.up } else { &mut self.down };
            for val in &mut arr[idx_start..idx_end] {
                *val += 1;
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
        for &v in map.up.iter() {
            assert_eq!(v, 0);
        }
        for &v in map.down.iter() {
            assert_eq!(v, 0);
        }
    }

    #[test]
    fn direct_index_access() {
        let mut map = CoverageMap::new(100, 102);
        map.up[1] = 42;
        assert_eq!(map.up[1], 42);
    }

    #[test]
    fn single_position_region() {
        let map = CoverageMap::new(100, 100);
        assert_eq!(map.len(), 1);
        assert_eq!(map.up[0], 0);
    }

    #[test]
    fn apply_match_span() {
        let mut map = CoverageMap::new(100, 109);
        let spans = vec![CigarSpan::Match { start: 102, len: 3 }];
        map.apply_spans(&spans);
        assert_eq!(map.up[1], 0); // pos 101
        assert_eq!(map.up[2], 1); // pos 102
        assert_eq!(map.up[3], 1); // pos 103
        assert_eq!(map.up[4], 1); // pos 104
        assert_eq!(map.up[5], 0); // pos 105
    }

    #[test]
    fn apply_skip_span() {
        let mut map = CoverageMap::new(100, 109);
        let spans = vec![CigarSpan::Skip { start: 103, len: 2 }];
        map.apply_spans(&spans);
        assert_eq!(map.down[3], 1); // pos 103
        assert_eq!(map.down[4], 1); // pos 104
        assert_eq!(map.down[5], 0); // pos 105
    }

    #[test]
    fn apply_spans_clips_to_region_bounds() {
        let mut map = CoverageMap::new(100, 104);
        let spans = vec![CigarSpan::Match {
            start: 103,
            len: 10,
        }];
        map.apply_spans(&spans);
        assert_eq!(map.up[3], 1); // pos 103
        assert_eq!(map.up[4], 1); // pos 104
    }

    #[test]
    fn apply_spans_before_region_ignored() {
        let mut map = CoverageMap::new(100, 104);
        let spans = vec![CigarSpan::Match { start: 95, len: 3 }];
        map.apply_spans(&spans);
        for &v in map.up.iter() {
            assert_eq!(v, 0);
        }
    }

    #[test]
    #[should_panic(expected = "start")]
    fn new_panics_when_start_greater_than_end() {
        CoverageMap::new(200, 100);
    }

    #[test]
    fn apply_spans_straddling_region_start() {
        let mut map = CoverageMap::new(100, 104);
        let spans = vec![CigarSpan::Match { start: 98, len: 5 }];
        map.apply_spans(&spans);
        assert_eq!(map.up[0], 1); // pos 100
        assert_eq!(map.up[1], 1); // pos 101
        assert_eq!(map.up[2], 1); // pos 102
    }
}
