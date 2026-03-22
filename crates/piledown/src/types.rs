use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

#[cfg(feature = "python")]
use pyo3::prelude::*;

#[cfg(feature = "cli")]
use clap::ValueEnum;

/// Library preparation protocol type.
/// See [Salmon docs](https://salmon.readthedocs.io/en/latest/library_type.html)
#[cfg_attr(feature = "python", pyclass(eq, eq_int, from_py_object))]
#[cfg_attr(feature = "cli", derive(ValueEnum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LibFragmentType {
    Isf,
    Isr,
}

/// Strand of the original transcript.
#[cfg_attr(feature = "python", pyclass(eq, eq_int, from_py_object))]
#[cfg_attr(feature = "cli", derive(ValueEnum))]
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, AsRefStr, EnumString, Serialize, Deserialize,
)]
pub enum Strand {
    #[serde(rename = "+")]
    #[strum(serialize = "+")]
    Forward,
    #[serde(rename = "-")]
    #[strum(serialize = "-")]
    Reverse,
    #[serde(rename = ".")]
    #[strum(serialize = ".")]
    Either,
}

#[cfg_attr(feature = "cli", derive(ValueEnum))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum OutputFormat {
    Tsv,
    Arrow,
    Parquet,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strand_as_ref_matches_serde() {
        // as_ref() comes from strum, serde uses its own rename.
        // These must agree or TSV vs Arrow output will diverge.
        for (variant, expected) in [(Strand::Forward, "+"), (Strand::Reverse, "-"), (Strand::Either, ".")] {
            assert_eq!(variant.as_ref(), expected, "as_ref mismatch for {:?}", variant);

            // Verify serde CSV serialization produces the same string
            let mut buf = Vec::new();
            {
                let mut wtr = csv::WriterBuilder::new()
                    .has_headers(false)
                    .from_writer(&mut buf);
                wtr.serialize(&variant).unwrap();
                wtr.flush().unwrap();
            }
            let csv_output = String::from_utf8(buf).unwrap();
            assert_eq!(csv_output.trim(), expected, "serde CSV mismatch for {:?}", variant);
        }
    }
}
