use anyhow::Result;
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
    pub fn new(seq: String, start: u64, end: u64, name: String, strand: Strand) -> Self {
        Self {
            seq,
            start,
            end,
            name,
            strand,
        }
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
    for result in csv_reader.deserialize() {
        regions.push(result?);
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
        assert_eq!(<[u8] as AsRef<[u8]>>::as_ref(region.name().as_ref()), b"chr1");
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
    fn read_regions_from_tsv() {
        let tsv = "seq\tstart\tend\tname\tstrand\nchr1\t17000\t25000\ttes1\t+\nchr1\t26000\t30000\ttes2\t-\n";
        let regions = read_regions_tsv(tsv.as_bytes()).unwrap();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].name, "tes1");
        assert_eq!(regions[1].strand, Strand::Reverse);
    }
}
