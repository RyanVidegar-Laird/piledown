#[cfg(feature = "async")]
mod async_engine {
    use std::path::{Path, PathBuf};

    use anyhow::Result;
    use futures::TryStreamExt;
    use noodles::bam;
    use noodles::bam::bai;
    use noodles::sam;
    use tokio::fs::File;

    use crate::cigar::cigar_spans;
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
        pub concurrency: usize,
    }

    /// Async BAM reader wrapping noodles.
    struct BamSource {
        reader: bam::r#async::io::Reader<noodles::bgzf::r#async::Reader<File>>,
        header: sam::Header,
        index: bai::Index,
    }

    impl BamSource {
        /// Open an indexed BAM file for async reading.
        async fn open(bam_path: impl AsRef<Path>) -> Result<Self> {
            let bam_path = bam_path.as_ref();
            let mut reader = File::open(bam_path)
                .await
                .map(bam::r#async::io::Reader::new)?;
            let header = reader.read_header().await?;
            let index = bai::r#async::read(bam_path.with_extension("bam.bai")).await?;
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

            let mut query = self
                .reader
                .query(&self.header, &self.index, &noodle_region)?;

            while let Some(record) = query.try_next().await? {
                let flags = record.flags();
                if !filter::apply_filters(flags, filters) {
                    continue;
                }

                if region.strand != Strand::Either {
                    match classifier.classify(flags) {
                        Ok(s) if s == region.strand => {}
                        Ok(_) => continue,
                        Err(_) => continue,
                    }
                }

                let alignment_start = match record.alignment_start() {
                    Some(Ok(pos)) => pos.get() as u64,
                    _ => continue,
                };

                let ops: Vec<_> = record.cigar().iter().filter_map(|op| op.ok()).collect();
                let spans = cigar_spans(alignment_start, &ops);
                map.apply_spans(&spans);
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
        async fn process_one(&self, region: PileRegion) -> Result<(PileRegion, CoverageMap)> {
            let mut source = BamSource::open(&self.config.bam_path).await?;
            let filters = self.build_filters();
            let classifier = self.build_classifier();
            let map = source
                .process_region(&region, &filters, classifier.as_ref())
                .await?;
            Ok((region, map))
        }

        /// Collect results for all regions into memory.
        pub async fn run_collect(
            &self,
            regions: Vec<PileRegion>,
        ) -> Result<Vec<(PileRegion, CoverageMap)>> {
            use futures::stream::{self, StreamExt};

            let results: Vec<Result<_>> = stream::iter(regions)
                .map(|region| self.process_one(region))
                .buffer_unordered(self.config.concurrency)
                .collect()
                .await;

            results.into_iter().collect()
        }

        /// Stream results, calling sink for each completed region.
        pub async fn run_streaming<F>(&self, regions: Vec<PileRegion>, mut sink: F) -> Result<()>
        where
            F: FnMut(PileRegion, CoverageMap) -> Result<()>,
        {
            use futures::stream::{self, StreamExt};

            let mut stream = stream::iter(regions)
                .map(|region| self.process_one(region))
                .buffer_unordered(self.config.concurrency);

            while let Some(result) = stream.next().await {
                let (region, map) = result?;
                sink(region, map)?;
            }
            Ok(())
        }
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
}
