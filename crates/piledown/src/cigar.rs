use noodles::sam::alignment::record::cigar::op::{Kind, Op};

/// Intermediate representation of reference-consuming CIGAR operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CigarSpan {
    /// M, =, X — aligned bases, contributes to "up" count
    Match { start: u64, len: u64 },
    /// D, N — deletions/skips, contributes to "down" count
    Skip { start: u64, len: u64 },
}

/// Walk a sequence of CIGAR ops starting at `alignment_start` and emit positioned spans.
/// Non-reference-consuming ops (I, S, H, P) are dropped.
pub fn cigar_spans(alignment_start: u64, ops: &[Op]) -> Vec<CigarSpan> {
    let mut spans = Vec::new();
    let mut pos = alignment_start;

    for op in ops {
        let len = op.len() as u64;
        match op.kind() {
            Kind::Match | Kind::SequenceMatch | Kind::SequenceMismatch => {
                spans.push(CigarSpan::Match { start: pos, len });
                pos += len;
            }
            Kind::Deletion | Kind::Skip => {
                spans.push(CigarSpan::Skip { start: pos, len });
                pos += len;
            }
            // Non-reference-consuming: I, S, H, P
            Kind::Insertion | Kind::SoftClip | Kind::HardClip | Kind::Pad => {}
        }
    }

    spans
}

/// Filter spans by anchor length. For each Skip span, check that the
/// immediately flanking Match spans are each >= `anchor_length`. If a Skip
/// fails, it is excluded along with any flanking Match span shorter than
/// the threshold. Match spans not adjacent to any Skip are never filtered.
///
/// Returns a new Vec containing only the surviving spans.
pub fn filter_spans_by_anchor(spans: &[CigarSpan], anchor_length: u64) -> Vec<CigarSpan> {
    if anchor_length == 0 || spans.is_empty() {
        return spans.to_vec();
    }

    let n = spans.len();

    // Phase 1: identify which Skip spans fail the anchor check.
    let mut skip_failed = vec![false; n];
    for i in 0..n {
        if matches!(spans[i], CigarSpan::Skip { .. }) {
            let left_ok = i > 0
                && matches!(spans[i - 1], CigarSpan::Match { len, .. } if len >= anchor_length);
            let right_ok = i + 1 < n
                && matches!(spans[i + 1], CigarSpan::Match { len, .. } if len >= anchor_length);
            if !left_ok || !right_ok {
                skip_failed[i] = true;
            }
        }
    }

    // Phase 2: build filtered list. A Match span is excluded only if it is
    // immediately adjacent to a failed Skip AND its own length < anchor_length.
    let mut result = Vec::with_capacity(n);
    for i in 0..n {
        match &spans[i] {
            CigarSpan::Skip { .. } => {
                if !skip_failed[i] {
                    result.push(spans[i].clone());
                }
            }
            CigarSpan::Match { len, .. } => {
                let adj_failed_left = i > 0 && skip_failed[i - 1];
                let adj_failed_right = i + 1 < n && skip_failed[i + 1];
                let dominated = (adj_failed_left || adj_failed_right) && *len < anchor_length;
                if !dominated {
                    result.push(spans[i].clone());
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(kind: Kind, len: usize) -> Op {
        Op::new(kind, len)
    }

    #[test]
    fn simple_match() {
        let ops = vec![op(Kind::Match, 10)];
        let spans = cigar_spans(100, &ops);
        assert_eq!(
            spans,
            vec![CigarSpan::Match {
                start: 100,
                len: 10
            }]
        );
    }

    #[test]
    fn match_then_skip_then_match() {
        let ops = vec![op(Kind::Match, 5), op(Kind::Skip, 3), op(Kind::Match, 5)];
        let spans = cigar_spans(100, &ops);
        assert_eq!(
            spans,
            vec![
                CigarSpan::Match { start: 100, len: 5 },
                CigarSpan::Skip { start: 105, len: 3 },
                CigarSpan::Match { start: 108, len: 5 },
            ]
        );
    }

    #[test]
    fn insertion_does_not_consume_reference() {
        let ops = vec![
            op(Kind::Match, 5),
            op(Kind::Insertion, 2),
            op(Kind::Match, 5),
        ];
        let spans = cigar_spans(100, &ops);
        assert_eq!(
            spans,
            vec![
                CigarSpan::Match { start: 100, len: 5 },
                CigarSpan::Match { start: 105, len: 5 },
            ]
        );
    }

    #[test]
    fn soft_hard_clip_ignored() {
        let ops = vec![
            op(Kind::SoftClip, 3),
            op(Kind::Match, 5),
            op(Kind::HardClip, 2),
        ];
        let spans = cigar_spans(100, &ops);
        assert_eq!(spans, vec![CigarSpan::Match { start: 100, len: 5 }]);
    }

    #[test]
    fn deletion_produces_skip_span() {
        let ops = vec![
            op(Kind::Match, 5),
            op(Kind::Deletion, 2),
            op(Kind::Match, 5),
        ];
        let spans = cigar_spans(100, &ops);
        assert_eq!(
            spans,
            vec![
                CigarSpan::Match { start: 100, len: 5 },
                CigarSpan::Skip { start: 105, len: 2 },
                CigarSpan::Match { start: 107, len: 5 },
            ]
        );
    }

    #[test]
    fn sequence_match_and_mismatch() {
        let ops = vec![op(Kind::SequenceMatch, 3), op(Kind::SequenceMismatch, 2)];
        let spans = cigar_spans(100, &ops);
        assert_eq!(
            spans,
            vec![
                CigarSpan::Match { start: 100, len: 3 },
                CigarSpan::Match { start: 103, len: 2 },
            ]
        );
    }

    #[test]
    fn empty_ops() {
        let spans = cigar_spans(100, &[]);
        assert!(spans.is_empty());
    }

    // --- Anchor filtering tests ---

    #[test]
    fn anchor_zero_returns_unchanged() {
        let spans = vec![
            CigarSpan::Match {
                start: 100,
                len: 10,
            },
            CigarSpan::Skip {
                start: 110,
                len: 500,
            },
            CigarSpan::Match {
                start: 610,
                len: 15,
            },
        ];
        assert_eq!(filter_spans_by_anchor(&spans, 0), spans);
    }

    #[test]
    fn anchor_no_skips_unchanged() {
        let spans = vec![CigarSpan::Match {
            start: 100,
            len: 50,
        }];
        assert_eq!(filter_spans_by_anchor(&spans, 5), spans);
    }

    #[test]
    fn anchor_single_junction_passes() {
        // 10M500N15M, anchor=5 → all kept
        let spans = vec![
            CigarSpan::Match {
                start: 100,
                len: 10,
            },
            CigarSpan::Skip {
                start: 110,
                len: 500,
            },
            CigarSpan::Match {
                start: 610,
                len: 15,
            },
        ];
        assert_eq!(filter_spans_by_anchor(&spans, 5), spans);
    }

    #[test]
    fn anchor_single_junction_left_fails() {
        // 3M500N15M, anchor=5 → Skip and left Match excluded
        let spans = vec![
            CigarSpan::Match { start: 100, len: 3 },
            CigarSpan::Skip {
                start: 103,
                len: 500,
            },
            CigarSpan::Match {
                start: 603,
                len: 15,
            },
        ];
        let expected = vec![CigarSpan::Match {
            start: 603,
            len: 15,
        }];
        assert_eq!(filter_spans_by_anchor(&spans, 5), expected);
    }

    #[test]
    fn anchor_single_junction_right_fails() {
        // 15M500N3M, anchor=5 → Skip and right Match excluded
        let spans = vec![
            CigarSpan::Match {
                start: 100,
                len: 15,
            },
            CigarSpan::Skip {
                start: 115,
                len: 500,
            },
            CigarSpan::Match { start: 615, len: 3 },
        ];
        let expected = vec![CigarSpan::Match {
            start: 100,
            len: 15,
        }];
        assert_eq!(filter_spans_by_anchor(&spans, 5), expected);
    }

    #[test]
    fn anchor_both_sides_fail() {
        // 3M500N3M, anchor=5 → all excluded
        let spans = vec![
            CigarSpan::Match { start: 100, len: 3 },
            CigarSpan::Skip {
                start: 103,
                len: 500,
            },
            CigarSpan::Match { start: 603, len: 3 },
        ];
        let expected: Vec<CigarSpan> = vec![];
        assert_eq!(filter_spans_by_anchor(&spans, 5), expected);
    }

    #[test]
    fn anchor_multi_junction_middle_fails() {
        // 30M500N3M200N30M, anchor=5
        let spans = vec![
            CigarSpan::Match {
                start: 100,
                len: 30,
            },
            CigarSpan::Skip {
                start: 130,
                len: 500,
            },
            CigarSpan::Match { start: 630, len: 3 },
            CigarSpan::Skip {
                start: 633,
                len: 200,
            },
            CigarSpan::Match {
                start: 833,
                len: 30,
            },
        ];
        let expected = vec![
            CigarSpan::Match {
                start: 100,
                len: 30,
            },
            CigarSpan::Match {
                start: 833,
                len: 30,
            },
        ];
        assert_eq!(filter_spans_by_anchor(&spans, 5), expected);
    }

    #[test]
    fn anchor_adjacent_skips_both_fail() {
        // 10M100N200N10M, anchor=5
        let spans = vec![
            CigarSpan::Match {
                start: 100,
                len: 10,
            },
            CigarSpan::Skip {
                start: 110,
                len: 100,
            },
            CigarSpan::Skip {
                start: 210,
                len: 200,
            },
            CigarSpan::Match {
                start: 410,
                len: 10,
            },
        ];
        let expected = vec![
            CigarSpan::Match {
                start: 100,
                len: 10,
            },
            CigarSpan::Match {
                start: 410,
                len: 10,
            },
        ];
        assert_eq!(filter_spans_by_anchor(&spans, 5), expected);
    }

    #[test]
    fn anchor_skip_at_start_fails() {
        let spans = vec![
            CigarSpan::Skip {
                start: 100,
                len: 500,
            },
            CigarSpan::Match {
                start: 600,
                len: 20,
            },
        ];
        let expected = vec![CigarSpan::Match {
            start: 600,
            len: 20,
        }];
        assert_eq!(filter_spans_by_anchor(&spans, 5), expected);
    }

    #[test]
    fn anchor_skip_at_end_fails() {
        let spans = vec![
            CigarSpan::Match {
                start: 100,
                len: 20,
            },
            CigarSpan::Skip {
                start: 120,
                len: 500,
            },
        ];
        let expected = vec![CigarSpan::Match {
            start: 100,
            len: 20,
        }];
        assert_eq!(filter_spans_by_anchor(&spans, 5), expected);
    }

    #[test]
    fn anchor_deletion_with_long_flanks_passes() {
        // 20M2D20M, anchor=5 → all kept (D treated as Skip)
        let spans = vec![
            CigarSpan::Match {
                start: 100,
                len: 20,
            },
            CigarSpan::Skip { start: 120, len: 2 },
            CigarSpan::Match {
                start: 122,
                len: 20,
            },
        ];
        assert_eq!(filter_spans_by_anchor(&spans, 5), spans);
    }

    #[test]
    fn anchor_insertion_splits_match_conservative() {
        // 5M2I5M500N10M → [Match(5), Match(5), Skip(500), Match(10)]
        let spans = vec![
            CigarSpan::Match { start: 100, len: 5 },
            CigarSpan::Match { start: 105, len: 5 },
            CigarSpan::Skip {
                start: 110,
                len: 500,
            },
            CigarSpan::Match {
                start: 610,
                len: 10,
            },
        ];
        assert_eq!(filter_spans_by_anchor(&spans, 5), spans);
    }

    #[test]
    fn anchor_insertion_splits_match_too_short() {
        // 3M2I3M500N10M → [Match(3), Match(3), Skip(500), Match(10)]
        let spans = vec![
            CigarSpan::Match { start: 100, len: 3 },
            CigarSpan::Match { start: 103, len: 3 },
            CigarSpan::Skip {
                start: 106,
                len: 500,
            },
            CigarSpan::Match {
                start: 606,
                len: 10,
            },
        ];
        let expected = vec![
            CigarSpan::Match { start: 100, len: 3 },
            CigarSpan::Match {
                start: 606,
                len: 10,
            },
        ];
        assert_eq!(filter_spans_by_anchor(&spans, 5), expected);
    }

    #[test]
    fn anchor_exact_boundary_passes() {
        // 5M500N15M, anchor=5 → len=5 exactly meets threshold
        let spans = vec![
            CigarSpan::Match { start: 100, len: 5 },
            CigarSpan::Skip {
                start: 105,
                len: 500,
            },
            CigarSpan::Match {
                start: 605,
                len: 15,
            },
        ];
        assert_eq!(filter_spans_by_anchor(&spans, 5), spans);
    }

    #[test]
    fn anchor_empty_spans() {
        let spans: Vec<CigarSpan> = vec![];
        assert_eq!(filter_spans_by_anchor(&spans, 5), spans);
    }
}
