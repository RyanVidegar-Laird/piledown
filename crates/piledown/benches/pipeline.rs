use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, Criterion};
use futures::stream::StreamExt;
use piledown::engine::{runtime, EngineConfig, PileEngine};
use piledown::region::PileRegion;
use piledown::types::{LibFragmentType, Strand};

fn bench_full_pipeline(c: &mut Criterion) {
    let bam_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/data/SRR21778056-sorted-subsample.bam");
    let region =
        PileRegion::new("chr1".into(), 14900, 15200, "bench".into(), Strand::Reverse).unwrap();

    let rt = runtime();

    c.bench_function("full_pipeline_single_region", |b| {
        b.iter(|| {
            let config = EngineConfig {
                bam_path: bam_path.clone(),
                exclude_flags: None,
                lib_type: LibFragmentType::Isr,
                concurrency: 1,
                index_path: None,
                chunk_size: None,
            };
            let engine = PileEngine::new(config);
            rt.block_on(async {
                let stream = std::pin::pin!(engine.run(vec![region.clone()]));
                stream
                    .collect::<Vec<_>>()
                    .await
                    .into_iter()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap()
            });
        });
    });
}

criterion_group!(benches, bench_full_pipeline);
criterion_main!(benches);
