use anyhow::{anyhow, Result};
use noodles::core::Region;
use serde::{Deserialize, Serialize};

use crate::types::Strand;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct JunctionRegion {
    pub seq: String,
    pub start: u64,
    pub end: u64,
    pub name: String,
    pub strand: Strand,
    #[serde(default, alias = "anchor")]
    pub anchor_length: Option<u64>,
}

impl JunctionRegion {
    pub fn new(seq: String, start: u64, end: u64, name: String, strand: Strand) -> Result<Self> {
        if start >= end {
            return Err(anyhow!(
                "junction start ({start}) must be < end ({end})"
            ));
        }
        Ok(Self {
            seq,
            start,
            end,
            name,
            strand,
            anchor_length: None,
        })
    }
}

impl TryFrom<JunctionRegion> for Region {
    type Error = anyhow::Error;
    fn try_from(jr: JunctionRegion) -> Result<Region, Self::Error> {
        let region_str = format!("{}:{}-{}", jr.seq, jr.start, jr.end);
        let region: Region = region_str.parse()?;
        Ok(region)
    }
}

/// Parse junctions from a TSV file (columns: seq, start, end, name, strand, [anchor]).
pub fn read_junctions_tsv(reader: impl std::io::Read) -> Result<Vec<JunctionRegion>> {
    let mut csv_reader = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .has_headers(true)
        .from_reader(reader);
    let mut junctions = Vec::new();
    for (i, result) in csv_reader.deserialize().enumerate() {
        let junction: JunctionRegion = result?;
        if junction.start >= junction.end {
            return Err(anyhow!(
                "junction at row {} has start ({}) >= end ({})",
                i + 1,
                junction.start,
                junction.end
            ));
        }
        junctions.push(junction);
    }
    Ok(junctions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_valid() {
        let jr = JunctionRegion::new("chr1".into(), 100, 500, "junc1".into(), Strand::Forward);
        assert!(jr.is_ok());
        let jr = jr.unwrap();
        assert_eq!(jr.seq, "chr1");
        assert_eq!(jr.start, 100);
        assert_eq!(jr.end, 500);
    }

    #[test]
    fn new_rejects_start_equals_end() {
        assert!(JunctionRegion::new("chr1".into(), 100, 100, "j".into(), Strand::Forward).is_err());
    }

    #[test]
    fn new_rejects_start_greater_than_end() {
        assert!(JunctionRegion::new("chr1".into(), 500, 100, "j".into(), Strand::Forward).is_err());
    }

    #[test]
    fn to_noodles_region() {
        let jr =
            JunctionRegion::new("chr1".into(), 100, 500, "j".into(), Strand::Forward).unwrap();
        let region: Region = jr.try_into().unwrap();
        assert_eq!(
            <[u8] as AsRef<[u8]>>::as_ref(region.name().as_ref()),
            b"chr1"
        );
    }

    #[test]
    fn read_junctions_tsv_basic() {
        let tsv = "seq\tstart\tend\tname\tstrand\nchr1\t100\t500\tjunc1\t+\nchr1\t2000\t3000\tjunc2\t-\n";
        let junctions = super::read_junctions_tsv(tsv.as_bytes()).unwrap();
        assert_eq!(junctions.len(), 2);
        assert_eq!(junctions[0].name, "junc1");
        assert_eq!(junctions[1].strand, Strand::Reverse);
    }

    #[test]
    fn read_junctions_tsv_rejects_start_equals_end() {
        let tsv = "seq\tstart\tend\tname\tstrand\nchr1\t100\t100\tjunc1\t+\n";
        assert!(super::read_junctions_tsv(tsv.as_bytes()).is_err());
    }

    #[test]
    fn read_junctions_tsv_with_anchor() {
        let tsv =
            "seq\tstart\tend\tname\tstrand\tanchor\nchr1\t100\t500\tjunc1\t+\t8\n";
        let junctions = super::read_junctions_tsv(tsv.as_bytes()).unwrap();
        assert_eq!(junctions[0].anchor_length, Some(8));
    }
}
