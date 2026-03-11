use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use noodles::sam::alignment::record::cigar::op::{Kind, Op};
use piledown::cigar::{cigar_spans, CigarSpan};
use piledown::coverage::CoverageMap;

fn bench_apply_spans(c: &mut Criterion) {
    let mut group = c.benchmark_group("apply_spans");

    for region_size in [1_000u64, 10_000, 100_000] {
        let spans: Vec<CigarSpan> = (0..region_size / 100)
            .map(|i| CigarSpan::Match {
                start: i * 100,
                len: 100,
            })
            .collect();

        group.bench_with_input(
            BenchmarkId::from_parameter(region_size),
            &region_size,
            |b, &size| {
                b.iter(|| {
                    let mut map = CoverageMap::new(0, size - 1);
                    map.apply_spans(&spans);
                    map
                });
            },
        );
    }
    group.finish();
}

fn bench_cigar_spans(c: &mut Criterion) {
    let ops: Vec<Op> = vec![
        Op::new(Kind::Match, 50),
        Op::new(Kind::Skip, 5000),
        Op::new(Kind::Match, 50),
        Op::new(Kind::Skip, 3000),
        Op::new(Kind::Match, 50),
    ];

    c.bench_function("cigar_spans_spliced_read", |b| {
        b.iter(|| cigar_spans(1000, &ops));
    });
}

criterion_group!(benches, bench_apply_spans, bench_cigar_spans);
criterion_main!(benches);
