#![cfg(feature = "async")]

use std::collections::HashMap;
use std::path::PathBuf;

use futures::stream::StreamExt;
use piledown::engine::{EngineConfig, PileEngine};
use piledown::output::to_record_batch;
use piledown::region::PileRegion;
use piledown::types::{LibFragmentType, Strand};

fn test_bam() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/data/SRR21778056-sorted-subsample.bam")
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/golden")
}

/// Parse a golden fixture TSV into a map of pos -> (up, down).
/// Handles both old 5-col (seq, pos, strand, up, down) and
/// new 6-col (name, seq, pos, strand, up, down) formats.
fn parse_golden(path: &std::path::Path) -> HashMap<u64, (u64, u64)> {
    let content = std::fs::read_to_string(path).unwrap();
    let mut lines = content.lines();
    let header = lines.next().unwrap();
    let cols: Vec<&str> = header.split('\t').collect();

    // Determine column indices for pos, up, down
    let pos_idx = cols.iter().position(|c| *c == "pos").unwrap();
    let up_idx = cols.iter().position(|c| *c == "up").unwrap();
    let down_idx = cols.iter().position(|c| *c == "down").unwrap();

    let mut map = HashMap::new();
    for line in lines {
        let fields: Vec<&str> = line.split('\t').collect();
        let pos: u64 = fields[pos_idx].parse().unwrap();
        let up: u64 = fields[up_idx].parse().unwrap();
        let down: u64 = fields[down_idx].parse().unwrap();
        map.insert(pos, (up, down));
    }
    map
}

/// Extract pos -> (up, down) from a RecordBatch.
fn batch_to_coverage_map(batch: &arrow::array::RecordBatch) -> HashMap<u64, (u64, u64)> {
    let pos_col = batch
        .column(3)
        .as_any()
        .downcast_ref::<arrow::array::UInt64Array>()
        .unwrap();
    let up_col = batch
        .column(4)
        .as_any()
        .downcast_ref::<arrow::array::UInt64Array>()
        .unwrap();
    let down_col = batch
        .column(5)
        .as_any()
        .downcast_ref::<arrow::array::UInt64Array>()
        .unwrap();

    let mut map = HashMap::new();
    for i in 0..batch.num_rows() {
        map.insert(pos_col.value(i), (up_col.value(i), down_col.value(i)));
    }
    map
}

#[tokio::test]
async fn single_region_isr_reverse_matches_golden() {
    let region =
        PileRegion::new("chr1".into(), 14900, 15200, "test".into(), Strand::Reverse).unwrap();

    let config = EngineConfig {
        bam_path: test_bam(),
        exclude_flags: None,
        lib_type: LibFragmentType::Isr,
        concurrency: 1,
        index_path: None,
        chunk_size: None,
    };

    let engine = PileEngine::new(config);
    let mut stream = std::pin::pin!(engine.run(vec![region]));
    let (region, map) = stream.next().await.unwrap().unwrap();

    let batch = to_record_batch(region, map).unwrap();
    assert_eq!(batch.num_rows(), 301); // positions 14900..=15200

    let golden_path = golden_dir().join("chr1_14900-15200_isr_reverse.tsv");
    assert!(golden_path.exists(), "golden fixture not found");

    let golden = parse_golden(&golden_path);
    let actual = batch_to_coverage_map(&batch);

    assert_eq!(
        golden.len(),
        actual.len(),
        "row count mismatch: golden={}, actual={}",
        golden.len(),
        actual.len()
    );

    for (pos, (g_up, g_down)) in &golden {
        let (a_up, a_down) = actual
            .get(pos)
            .unwrap_or_else(|| panic!("position {pos} missing from actual output"));
        assert_eq!(
            (a_up, a_down),
            (g_up, g_down),
            "mismatch at pos {pos}: actual=({a_up},{a_down}) golden=({g_up},{g_down})"
        );
    }
}

#[tokio::test]
async fn single_region_isr_forward_matches_golden() {
    let region =
        PileRegion::new("chr1".into(), 17000, 17500, "test".into(), Strand::Forward).unwrap();

    let config = EngineConfig {
        bam_path: test_bam(),
        exclude_flags: None,
        lib_type: LibFragmentType::Isr,
        concurrency: 1,
        index_path: None,
        chunk_size: None,
    };

    let engine = PileEngine::new(config);
    let mut stream = std::pin::pin!(engine.run(vec![region]));
    let (region, map) = stream.next().await.unwrap().unwrap();

    let batch = to_record_batch(region, map).unwrap();
    assert_eq!(batch.num_rows(), 501); // positions 17000..=17500

    let golden_path = golden_dir().join("chr1_17000-17500_isr_forward.tsv");
    assert!(golden_path.exists(), "golden fixture not found");

    let golden = parse_golden(&golden_path);
    let actual = batch_to_coverage_map(&batch);

    assert_eq!(golden.len(), actual.len());

    for (pos, (g_up, g_down)) in &golden {
        let (a_up, a_down) = actual.get(pos).unwrap();
        assert_eq!(
            (a_up, a_down),
            (g_up, g_down),
            "mismatch at pos {pos}: actual=({a_up},{a_down}) golden=({g_up},{g_down})"
        );
    }
}

#[tokio::test]
async fn multi_region_validates_against_golden() {
    let regions = vec![
        PileRegion::new("chr1".into(), 14900, 15200, "r1".into(), Strand::Reverse).unwrap(),
        PileRegion::new("chr1".into(), 17000, 17500, "r2".into(), Strand::Forward).unwrap(),
    ];

    let config = EngineConfig {
        bam_path: test_bam(),
        exclude_flags: None,
        lib_type: LibFragmentType::Isr,
        concurrency: 2,
        index_path: None,
        chunk_size: None,
    };

    let engine = PileEngine::new(config);
    let results: Vec<(PileRegion, piledown::coverage::CoverageMap)> = {
        let stream = std::pin::pin!(engine.run(regions));
        stream
            .collect::<Vec<anyhow::Result<_>>>()
            .await
            .into_iter()
            .collect::<anyhow::Result<Vec<_>>>()
            .unwrap()
    };
    assert_eq!(results.len(), 2);

    // Build lookup by region name (still needed since we validate by name)
    let by_name: HashMap<&str, _> = results
        .iter()
        .map(|(r, m)| (r.name.as_str(), (r, m)))
        .collect();

    // Validate r1 (chr1:14900-15200, reverse) against golden
    let (r1_region, r1_map) = by_name["r1"];
    let r1_batch = to_record_batch(r1_region.clone(), r1_map.clone()).unwrap();
    let r1_golden = parse_golden(&golden_dir().join("chr1_14900-15200_isr_reverse.tsv"));
    let r1_actual = batch_to_coverage_map(&r1_batch);
    assert_eq!(r1_golden.len(), r1_actual.len(), "r1 row count mismatch");
    for (pos, (g_up, g_down)) in &r1_golden {
        let (a_up, a_down) = r1_actual.get(pos).unwrap();
        assert_eq!((a_up, a_down), (g_up, g_down), "r1 mismatch at pos {pos}");
    }

    // Validate r2 (chr1:17000-17500, forward) against golden
    let (r2_region, r2_map) = by_name["r2"];
    let r2_batch = to_record_batch(r2_region.clone(), r2_map.clone()).unwrap();
    let r2_golden = parse_golden(&golden_dir().join("chr1_17000-17500_isr_forward.tsv"));
    let r2_actual = batch_to_coverage_map(&r2_batch);
    assert_eq!(r2_golden.len(), r2_actual.len(), "r2 row count mismatch");
    for (pos, (g_up, g_down)) in &r2_golden {
        let (a_up, a_down) = r2_actual.get(pos).unwrap();
        assert_eq!((a_up, a_down), (g_up, g_down), "r2 mismatch at pos {pos}");
    }
}

#[tokio::test]
async fn single_region_isr_either_matches_golden() {
    let region =
        PileRegion::new("chr1".into(), 14900, 15200, "test".into(), Strand::Either).unwrap();

    let config = EngineConfig {
        bam_path: test_bam(),
        exclude_flags: None,
        lib_type: LibFragmentType::Isr,
        concurrency: 1,
        index_path: None,
        chunk_size: None,
    };

    let engine = PileEngine::new(config);
    let mut stream = std::pin::pin!(engine.run(vec![region]));
    let (region, map) = stream.next().await.unwrap().unwrap();

    let batch = to_record_batch(region, map).unwrap();
    assert_eq!(batch.num_rows(), 301);

    let golden_path = golden_dir().join("chr1_14900-15200_isr_either.tsv");
    assert!(golden_path.exists(), "golden fixture not found");

    let golden = parse_golden(&golden_path);
    let actual = batch_to_coverage_map(&batch);

    assert_eq!(
        golden.len(),
        actual.len(),
        "row count mismatch: golden={}, actual={}",
        golden.len(),
        actual.len()
    );

    for (pos, (g_up, g_down)) in &golden {
        let (a_up, a_down) = actual
            .get(pos)
            .unwrap_or_else(|| panic!("position {pos} missing from actual output"));
        assert_eq!(
            (a_up, a_down),
            (g_up, g_down),
            "mismatch at pos {pos}: actual=({a_up},{a_down}) golden=({g_up},{g_down})"
        );
    }
}

#[tokio::test]
async fn missing_bam_returns_error() {
    let region = PileRegion::new("chr1".into(), 100, 200, "test".into(), Strand::Forward).unwrap();

    let config = EngineConfig {
        bam_path: "/nonexistent/path.bam".into(),
        exclude_flags: None,
        lib_type: LibFragmentType::Isr,
        concurrency: 1,
        index_path: None,
        chunk_size: None,
    };

    let engine = PileEngine::new(config);
    let mut stream = std::pin::pin!(engine.run(vec![region]));
    let result = stream.next().await.unwrap();
    assert!(result.is_err());
}

#[tokio::test]
async fn empty_region_returns_all_zeros() {
    let region = PileRegion::new("chr1".into(), 1, 100, "empty".into(), Strand::Either).unwrap();

    let config = EngineConfig {
        bam_path: test_bam(),
        exclude_flags: None,
        lib_type: LibFragmentType::Isr,
        concurrency: 1,
        index_path: None,
        chunk_size: None,
    };

    let engine = PileEngine::new(config);
    let mut stream = std::pin::pin!(engine.run(vec![region]));
    let (region, map) = stream.next().await.unwrap().unwrap();

    assert_eq!(region.name, "empty");
    assert_eq!(map.len(), 100); // positions 1..=100
    assert!(
        map.up.iter().all(|&v| v == 0),
        "expected all up counts to be zero"
    );
    assert!(
        map.down.iter().all(|&v| v == 0),
        "expected all down counts to be zero"
    );
}

#[tokio::test]
async fn single_region_isf_reverse_matches_golden() {
    let region =
        PileRegion::new("chr1".into(), 14900, 15200, "test".into(), Strand::Reverse).unwrap();

    let config = EngineConfig {
        bam_path: test_bam(),
        exclude_flags: None,
        lib_type: LibFragmentType::Isf,
        concurrency: 1,
        index_path: None,
        chunk_size: None,
    };

    let engine = PileEngine::new(config);
    let mut stream = std::pin::pin!(engine.run(vec![region]));
    let (region, map) = stream.next().await.unwrap().unwrap();

    let batch = to_record_batch(region, map).unwrap();
    assert_eq!(batch.num_rows(), 301);

    let golden_path = golden_dir().join("chr1_14900-15200_isf_reverse.tsv");
    assert!(golden_path.exists(), "ISF golden fixture not found");

    let golden = parse_golden(&golden_path);
    let actual = batch_to_coverage_map(&batch);

    assert_eq!(
        golden.len(),
        actual.len(),
        "row count mismatch: golden={}, actual={}",
        golden.len(),
        actual.len()
    );

    for (pos, (g_up, g_down)) in &golden {
        let (a_up, a_down) = actual
            .get(pos)
            .unwrap_or_else(|| panic!("position {pos} missing from actual output"));
        assert_eq!(
            (a_up, a_down),
            (g_up, g_down),
            "mismatch at pos {pos}: actual=({a_up},{a_down}) golden=({g_up},{g_down})"
        );
    }
}
