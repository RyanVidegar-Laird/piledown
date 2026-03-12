use anyhow::{anyhow, Result};
use noodles::core::Region;
use serde::{Deserialize, Serialize};

use crate::types::Strand;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PileRegion {
    pub seq: String,
    pub start: u64,
    pub end: u64,
    pub name: String,
    pub strand: Strand,
}

impl PileRegion {
    pub fn new(seq: String, start: u64, end: u64, name: String, strand: Strand) -> Result<Self> {
        if start > end {
            return Err(anyhow!("region start ({}) must be <= end ({})", start, end));
        }
        Ok(Self {
            seq,
            start,
            end,
            name,
            strand,
        })
    }

    /// Parse a noodles region string (e.g. "chr1:1000-2000") into a PileRegion.
    pub fn from_region_str(region: &str, name: String, strand: Strand) -> Result<Self> {
        let parsed: Region = region
            .parse()
            .map_err(|e: noodles::core::region::ParseError| anyhow!(e))?;
        let seq = String::from_utf8(parsed.name().to_vec())
            .map_err(|e| anyhow!("non-UTF8 sequence name: {}", e))?;
        let interval = parsed.interval();
        let start = interval
            .start()
            .ok_or_else(|| anyhow!("region missing start"))?
            .get() as u64;
        let end = interval
            .end()
            .ok_or_else(|| anyhow!("region missing end"))?
            .get() as u64;
        Self::new(seq, start, end, name, strand)
    }
}

impl TryFrom<PileRegion> for Region {
    type Error = anyhow::Error;
    fn try_from(pr: PileRegion) -> Result<Region, Self::Error> {
        let region_str = format!("{}:{}-{}", pr.seq, pr.start, pr.end);
        let region: Region = region_str.parse()?;
        Ok(region)
    }
}

/// Parse regions from a TSV file (columns: seq, start, end, name, strand).
pub fn read_regions_tsv(reader: impl std::io::Read) -> Result<Vec<PileRegion>> {
    let mut csv_reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .from_reader(reader);
    let mut regions = Vec::new();
    for (i, result) in csv_reader.deserialize().enumerate() {
        let region: PileRegion = result?;
        if region.start > region.end {
            return Err(anyhow!(
                "region at row {} has start ({}) > end ({})",
                i + 1,
                region.start,
                region.end
            ));
        }
        regions.push(region);
    }
    Ok(regions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pile_region_to_noodles_region() {
        let pr = PileRegion {
            seq: "chr1".into(),
            start: 1000,
            end: 2000,
            name: "test".into(),
            strand: Strand::Forward,
        };
        let region: Region = pr.try_into().unwrap();
        assert_eq!(
            <[u8] as AsRef<[u8]>>::as_ref(region.name().as_ref()),
            b"chr1"
        );
    }

    #[test]
    fn deserialize_from_tsv_row() {
        let tsv = "chr1\t17000\t25000\ttes1\t+\n";
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .has_headers(false)
            .from_reader(tsv.as_bytes());
        let record: PileRegion = reader.deserialize().next().unwrap().unwrap();
        assert_eq!(record.seq, "chr1");
        assert_eq!(record.start, 17000);
        assert_eq!(record.end, 25000);
        assert_eq!(record.name, "tes1");
        assert_eq!(record.strand, Strand::Forward);
    }

    #[test]
    fn from_region_str_parses_correctly() {
        let pr =
            PileRegion::from_region_str("chr1:1000-2000", "test".into(), Strand::Forward).unwrap();
        assert_eq!(pr.seq, "chr1");
        assert_eq!(pr.start, 1000);
        assert_eq!(pr.end, 2000);
        assert_eq!(pr.name, "test");
        assert_eq!(pr.strand, Strand::Forward);
    }

    #[test]
    fn from_region_str_rejects_invalid() {
        assert!(PileRegion::from_region_str("invalid", "test".into(), Strand::Forward).is_err());
    }

    #[test]
    fn read_regions_from_tsv() {
        let tsv = "seq\tstart\tend\tname\tstrand\nchr1\t17000\t25000\ttes1\t+\nchr1\t26000\t30000\ttes2\t-\n";
        let regions = read_regions_tsv(tsv.as_bytes()).unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].name, "tes1");
        assert_eq!(regions[1].strand, Strand::Reverse);
    }

    #[test]
    fn new_rejects_start_greater_than_end() {
        assert!(PileRegion::new("chr1".into(), 200, 100, "test".into(), Strand::Forward).is_err());
    }

    #[test]
    fn from_region_str_no_range() {
        assert!(PileRegion::from_region_str("chr1", "test".into(), Strand::Forward).is_err());
    }

    #[test]
    fn from_region_str_inverted_range() {
        assert!(
            PileRegion::from_region_str("chr1:200-100", "test".into(), Strand::Forward).is_err()
        );
    }

    #[test]
    fn from_region_str_malformed() {
        assert!(PileRegion::from_region_str("chr1:100", "test".into(), Strand::Forward).is_err());
    }

    #[test]
    fn read_regions_tsv_empty_file() {
        let tsv = "";
        let regions = read_regions_tsv(tsv.as_bytes());
        if let Ok(r) = regions {
            assert!(r.is_empty());
        }
    }

    #[test]
    fn read_regions_tsv_header_only() {
        let tsv = "seq\tstart\tend\tname\tstrand\n";
        let regions = read_regions_tsv(tsv.as_bytes()).unwrap();
        assert!(regions.is_empty());
    }

    #[test]
    fn read_regions_tsv_inverted_range() {
        let tsv = "seq\tstart\tend\tname\tstrand\nchr1\t200\t100\ttest\t+\n";
        assert!(read_regions_tsv(tsv.as_bytes()).is_err());
    }

    #[test]
    fn read_regions_tsv_missing_column() {
        let tsv = "seq\tstart\tend\nchr1\t100\t200\n";
        assert!(read_regions_tsv(tsv.as_bytes()).is_err());
    }

    #[test]
    fn read_regions_tsv_wrong_type() {
        let tsv = "seq\tstart\tend\tname\tstrand\nchr1\tabc\t200\ttest\t+\n";
        assert!(read_regions_tsv(tsv.as_bytes()).is_err());
    }
}
