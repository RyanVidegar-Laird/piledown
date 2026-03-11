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
}
