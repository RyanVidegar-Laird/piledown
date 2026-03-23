#[cfg(feature = "async")]
mod async_engine {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use futures::TryStreamExt;
    use noodles::bam;
    use noodles::bam::bai;
    use noodles::sam;
    use tokio::fs::File;

    use crate::cigar::{cigar_spans, filter_spans_by_anchor};
    use crate::coverage::CoverageMap;
    use crate::filter::{self, RecordFilter};
    use crate::region::PileRegion;
    use crate::strand::StrandClassifier;
    use crate::types::Strand;

    /// Configuration for a piledown run.
    pub struct EngineConfig {
        pub bam_path: PathBuf,
        pub exclude_flags: Option<noodles::sam::alignment::record::Flags>,
        pub lib_type: crate::types::LibFragmentType,
        /// Maximum number of regions to process concurrently via buffered.
        pub concurrency: usize,
        /// Optional explicit path to the BAM index file (.bai).
        /// If None, tries <bam>.bam.bai then <bam_stem>.bai.
        pub index_path: Option<PathBuf>,
        /// Optional chunk size: if set, regions larger than this many positions
        /// are split into multiple (PileRegion, CoverageMap) pairs in the output stream.
        pub chunk_size: Option<usize>,
        /// Global default anchor length. 0 = no filtering.
        pub anchor_length: u64,
    }

    /// Async BAM reader wrapping noodles.
    struct BamSource {
        reader: bam::r#async::io::Reader<noodles::bgzf::r#async::io::Reader<File>>,
        header: sam::Header,
        index: bai::Index,
    }

    impl BamSource {
        /// Open an indexed BAM file for async reading.
        async fn open(bam_path: impl AsRef<Path>, index_path: Option<&Path>) -> Result<Self> {
            let bam_path = bam_path.as_ref();
            let mut reader = File::open(bam_path)
                .await
                .map(bam::r#async::io::Reader::new)?;
            let header = reader.read_header().await?;

            let index = if let Some(idx_path) = index_path {
                bai::r#async::fs::read(idx_path).await.map_err(|e| {
                    anyhow::anyhow!(
                        "BAM index not found at specified path {}: {}",
                        idx_path.display(),
                        e
                    )
                })?
            } else {
                let bai_path1 = bam_path.with_extension("bam.bai");
                let bai_path2 = bam_path.with_extension("bai");
                if bai_path1.exists() {
                    bai::r#async::fs::read(&bai_path1).await?
                } else if bai_path2.exists() {
                    bai::r#async::fs::read(&bai_path2).await?
                } else {
                    return Err(anyhow::anyhow!(
                        "BAM index not found. Tried:\n  {}\n  {}",
                        bai_path1.display(),
                        bai_path2.display()
                    ));
                }
            };

            Ok(Self {
                reader,
                header,
                index,
            })
        }

        /// Query a region and process all records into a CoverageMap.
        async fn process_region(
            &mut self,
            region: &PileRegion,
            filters: &[Box<dyn RecordFilter>],
            classifier: &dyn StrandClassifier,
        ) -> Result<CoverageMap> {
            let mut map = CoverageMap::new(region.start, region.end);
            let noodle_region: noodles::core::Region = region.clone().try_into()?;

            let query = self
                .reader
                .query(&self.header, &self.index, &noodle_region)?;

            let mut cigar_error_count: u64 = 0;
            let mut strand_skip_count: u64 = 0;

            let mut records = std::pin::pin!(query.records());
            while let Some(record) = records.try_next().await? {
                let flags = record.flags();
                if !filter::apply_filters(flags, filters) {
                    continue;
                }

                if region.strand != Strand::Either {
                    match classifier.classify(flags) {
                        Ok(s) if s == region.strand => {}
                        Ok(_) => continue,
                        Err(e) => {
                            if strand_skip_count == 0 {
                                log::warn!(
                                    "strand classification failed in region {} at alignment position {:?}: {}",
                                    region.name,
                                    record.alignment_start(),
                                    e
                                );
                            }
                            strand_skip_count += 1;
                            continue;
                        }
                    }
                }

                let alignment_start = match record.alignment_start() {
                    Some(Ok(pos)) => pos.get() as u64,
                    _ => continue,
                };

                let mut ops = Vec::new();
                for op_result in record.cigar().iter() {
                    match op_result {
                        Ok(op) => ops.push(op),
                        Err(e) => {
                            if cigar_error_count == 0 {
                                log::warn!(
                                    "CIGAR parse error in region {} at alignment position {}: {}",
                                    region.name,
                                    alignment_start,
                                    e
                                );
                            }
                            cigar_error_count += 1;
                        }
                    }
                }
                let spans = cigar_spans(alignment_start, &ops);
                let effective_anchor = region.anchor_length.unwrap_or(0);
                let spans = if effective_anchor > 0 {
                    filter_spans_by_anchor(&spans, effective_anchor)
                } else {
                    spans
                };
                map.apply_spans(&spans);
            }

            if cigar_error_count > 0 {
                log::warn!(
                    "{} CIGAR operation(s) failed to parse in region {}",
                    cigar_error_count,
                    region.name
                );
            }
            if strand_skip_count > 0 {
                log::warn!(
                    "{} read(s) skipped due to unclassifiable strand in region {}",
                    strand_skip_count,
                    region.name
                );
            }

            Ok(map)
        }
    }

    /// Multi-region coverage engine.
    pub struct PileEngine {
        config: EngineConfig,
    }

    impl PileEngine {
        pub fn new(config: EngineConfig) -> Self {
            assert!(
                config.concurrency > 0,
                "concurrency must be >= 1, got {}",
                config.concurrency
            );
            if let Some(cs) = config.chunk_size {
                assert!(cs > 0, "chunk_size must be >= 1, got {}", cs);
            }
            Self { config }
        }

        pub(crate) fn build_filters(&self) -> Vec<Box<dyn RecordFilter>> {
            let mut filters: Vec<Box<dyn RecordFilter>> = Vec::new();
            if let Some(flags) = self.config.exclude_flags {
                filters.push(Box::new(crate::filter::FlagFilter(flags)));
            }
            filters
        }

        pub(crate) fn build_classifier(&self) -> Box<dyn StrandClassifier> {
            match self.config.lib_type {
                crate::types::LibFragmentType::Isr => Box::new(crate::strand::IsrClassifier),
                crate::types::LibFragmentType::Isf => Box::new(crate::strand::IsfClassifier),
            }
        }

        /// Process a single region. Opens its own BamSource (index seeks aren't shareable).
        async fn process_one(&self, mut region: PileRegion) -> Result<(PileRegion, CoverageMap)> {
            // Resolve per-region anchor: use region-specific if set, else global default
            region.anchor_length = Some(region.anchor_length.unwrap_or(self.config.anchor_length));

            let mut source =
                BamSource::open(&self.config.bam_path, self.config.index_path.as_deref()).await?;
            let filters = self.build_filters();
            let classifier = self.build_classifier();
            let map = source
                .process_region(&region, &filters, classifier.as_ref())
                .await?;
            Ok((region, map))
        }

        /// Return a stream of (PileRegion, CoverageMap) results, one per region
        /// (or more if chunk_size splits a large region). Uses `buffered()` to
        /// preserve input order.
        pub fn run(
            &self,
            regions: Vec<PileRegion>,
        ) -> impl futures::Stream<Item = Result<(PileRegion, CoverageMap)>> + '_ {
            use futures::stream::{self, StreamExt};

            let chunk_size = self.config.chunk_size;

            stream::iter(regions)
                .map(|region| self.process_one(region))
                .buffered(self.config.concurrency)
                .flat_map(move |result| match result {
                    Err(e) => Box::pin(stream::once(async { Err(e) }))
                        as std::pin::Pin<Box<dyn futures::Stream<Item = Result<_>> + Send>>,
                    Ok((region, map)) => {
                        if let Some(cs) = chunk_size {
                            if map.len() > cs {
                                let chunks = chunk_coverage(region, map, cs);
                                return Box::pin(stream::iter(chunks.into_iter().map(Ok)))
                                    as std::pin::Pin<
                                        Box<dyn futures::Stream<Item = Result<_>> + Send>,
                                    >;
                            }
                        }
                        Box::pin(stream::once(async { Ok((region, map)) }))
                            as std::pin::Pin<Box<dyn futures::Stream<Item = Result<_>> + Send>>
                    }
                })
        }
    }

    /// Split a (PileRegion, CoverageMap) into chunks of at most `chunk_size` positions.
    pub(crate) fn chunk_coverage(
        region: PileRegion,
        map: CoverageMap,
        chunk_size: usize,
    ) -> Vec<(PileRegion, CoverageMap)> {
        let total = map.len();
        let mut chunks = Vec::new();
        let mut offset = 0;
        let mut up_remaining = map.up;
        let mut down_remaining = map.down;

        while offset < total {
            let this_chunk = chunk_size.min(total - offset);
            let chunk_start = map.start + offset as u64;
            let chunk_end = chunk_start + this_chunk as u64 - 1;

            let up_rest = up_remaining.split_off(this_chunk);
            let down_rest = down_remaining.split_off(this_chunk);

            let chunk_map = CoverageMap {
                start: chunk_start,
                end: chunk_end,
                up: up_remaining,
                down: down_remaining,
            };

            let chunk_region = PileRegion {
                seq: region.seq.clone(),
                start: chunk_start,
                end: chunk_end,
                name: region.name.clone(),
                strand: region.strand,
                anchor_length: region.anchor_length,
            };

            chunks.push((chunk_region, chunk_map));
            up_remaining = up_rest;
            down_remaining = down_rest;
            offset += this_chunk;
        }
        chunks
    }
}

#[cfg(feature = "async")]
pub use async_engine::*;

#[cfg(feature = "async")]
use std::sync::OnceLock;

#[cfg(feature = "async")]
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

#[cfg(feature = "async")]
pub fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime")
    })
}

#[cfg(test)]
#[cfg(feature = "async")]
mod tests {
    use super::*;
    use noodles::sam::alignment::record::Flags;

    #[test]
    fn build_filters_none_when_no_exclude() {
        let config = EngineConfig {
            bam_path: "dummy.bam".into(),
            exclude_flags: None,
            lib_type: crate::types::LibFragmentType::Isr,
            concurrency: 1,
            index_path: None,
            chunk_size: None,
            anchor_length: 0,
        };
        let engine = PileEngine::new(config);
        let filters = engine.build_filters();
        assert!(filters.is_empty());
    }

    #[test]
    fn build_filters_has_flag_filter_when_exclude_set() {
        let config = EngineConfig {
            bam_path: "dummy.bam".into(),
            exclude_flags: Some(Flags::UNMAPPED),
            lib_type: crate::types::LibFragmentType::Isr,
            concurrency: 1,
            index_path: None,
            chunk_size: None,
            anchor_length: 0,
        };
        let engine = PileEngine::new(config);
        let filters = engine.build_filters();
        assert_eq!(filters.len(), 1);
        assert!(!filters[0].keep_flags(Flags::UNMAPPED));
        assert!(filters[0].keep_flags(Flags::SEGMENTED));
    }

    #[test]
    fn build_classifier_isr() {
        let config = EngineConfig {
            bam_path: "dummy.bam".into(),
            exclude_flags: None,
            lib_type: crate::types::LibFragmentType::Isr,
            concurrency: 1,
            index_path: None,
            chunk_size: None,
            anchor_length: 0,
        };
        let engine = PileEngine::new(config);
        let classifier = engine.build_classifier();
        // ISR Read1 reverse → Forward
        let f = Flags::from(0x1_u16 | 0x2 | 0x10 | 0x40);
        assert_eq!(
            classifier.classify(f).unwrap(),
            crate::types::Strand::Forward
        );
    }

    #[test]
    fn build_classifier_isf() {
        let config = EngineConfig {
            bam_path: "dummy.bam".into(),
            exclude_flags: None,
            lib_type: crate::types::LibFragmentType::Isf,
            concurrency: 1,
            index_path: None,
            chunk_size: None,
            anchor_length: 0,
        };
        let engine = PileEngine::new(config);
        let classifier = engine.build_classifier();
        // ISF Read1 reverse → Reverse (mirror of ISR)
        let f = Flags::from(0x1_u16 | 0x2 | 0x10 | 0x40);
        assert_eq!(
            classifier.classify(f).unwrap(),
            crate::types::Strand::Reverse
        );
    }

    #[test]
    fn chunk_coverage_splits_correctly() {
        use crate::coverage::CoverageMap;
        let region = crate::region::PileRegion::new(
            "chr1".into(),
            100,
            109,
            "test".into(),
            crate::types::Strand::Forward,
        )
        .unwrap();
        let mut map = CoverageMap::new(100, 109);
        map.up[0] = 1; // pos 100
        map.up[5] = 5; // pos 105
        map.up[9] = 9; // pos 109

        let chunks = chunk_coverage(region, map, 4);

        assert_eq!(chunks.len(), 3); // 10 positions / 4 = 3 chunks (4, 4, 2)

        // Chunk 0: positions 100-103
        assert_eq!(chunks[0].0.start, 100);
        assert_eq!(chunks[0].0.end, 103);
        assert_eq!(chunks[0].1.len(), 4);
        assert_eq!(chunks[0].1.up[0], 1);

        // Chunk 1: positions 104-107
        assert_eq!(chunks[1].0.start, 104);
        assert_eq!(chunks[1].0.end, 107);
        assert_eq!(chunks[1].1.len(), 4);
        assert_eq!(chunks[1].1.up[1], 5); // pos 105, offset 1 within chunk

        // Chunk 2: positions 108-109
        assert_eq!(chunks[2].0.start, 108);
        assert_eq!(chunks[2].0.end, 109);
        assert_eq!(chunks[2].1.len(), 2);
        assert_eq!(chunks[2].1.up[1], 9); // pos 109, offset 1 within chunk
    }

    #[test]
    fn chunk_coverage_size_one() {
        use crate::coverage::CoverageMap;
        let region = crate::region::PileRegion::new(
            "chr1".into(),
            100,
            102,
            "test".into(),
            crate::types::Strand::Forward,
        )
        .unwrap();
        let mut map = CoverageMap::new(100, 102);
        map.up[0] = 1;
        map.up[1] = 2;
        map.up[2] = 3;

        let chunks = chunk_coverage(region, map, 1);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].0.start, 100);
        assert_eq!(chunks[0].0.end, 100);
        assert_eq!(chunks[0].1.up[0], 1);
        assert_eq!(chunks[1].0.start, 101);
        assert_eq!(chunks[1].0.end, 101);
        assert_eq!(chunks[1].1.up[0], 2);
        assert_eq!(chunks[2].0.start, 102);
        assert_eq!(chunks[2].0.end, 102);
        assert_eq!(chunks[2].1.up[0], 3);
    }

    #[test]
    fn chunk_coverage_size_equal_to_region() {
        use crate::coverage::CoverageMap;
        let region = crate::region::PileRegion::new(
            "chr1".into(),
            100,
            104,
            "test".into(),
            crate::types::Strand::Forward,
        )
        .unwrap();
        let mut map = CoverageMap::new(100, 104);
        map.up[0] = 1;
        map.up[4] = 5;

        let chunks = chunk_coverage(region, map, 5);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0.start, 100);
        assert_eq!(chunks[0].0.end, 104);
        assert_eq!(chunks[0].1.len(), 5);
        assert_eq!(chunks[0].1.up[0], 1);
        assert_eq!(chunks[0].1.up[4], 5);
    }

    #[test]
    fn chunk_coverage_size_larger_than_region() {
        use crate::coverage::CoverageMap;
        let region = crate::region::PileRegion::new(
            "chr1".into(),
            100,
            102,
            "test".into(),
            crate::types::Strand::Forward,
        )
        .unwrap();
        let map = CoverageMap::new(100, 102);

        let chunks = chunk_coverage(region, map, 1000);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].0.start, 100);
        assert_eq!(chunks[0].0.end, 102);
        assert_eq!(chunks[0].1.len(), 3);
    }

    #[test]
    #[should_panic(expected = "concurrency")]
    fn rejects_zero_concurrency() {
        let config = EngineConfig {
            bam_path: "dummy.bam".into(),
            exclude_flags: None,
            lib_type: crate::types::LibFragmentType::Isr,
            concurrency: 0,
            index_path: None,
            chunk_size: None,
            anchor_length: 0,
        };
        PileEngine::new(config);
    }

    #[test]
    #[should_panic(expected = "chunk_size")]
    fn rejects_zero_chunk_size() {
        let config = EngineConfig {
            bam_path: "dummy.bam".into(),
            exclude_flags: None,
            lib_type: crate::types::LibFragmentType::Isr,
            concurrency: 1,
            index_path: None,
            chunk_size: Some(0),
            anchor_length: 0,
        };
        PileEngine::new(config);
    }

    #[tokio::test]
    async fn run_stream_yields_results() {
        use futures::stream::StreamExt;

        let config = EngineConfig {
            bam_path: "dummy.bam".into(),
            exclude_flags: None,
            lib_type: crate::types::LibFragmentType::Isr,
            concurrency: 1,
            index_path: None,
            chunk_size: None,
            anchor_length: 0,
        };
        let engine = PileEngine::new(config);
        let mut stream = std::pin::pin!(engine.run(vec![]));
        assert!(stream.next().await.is_none());
    }
}
