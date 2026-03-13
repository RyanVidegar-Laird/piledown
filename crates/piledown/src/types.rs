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
