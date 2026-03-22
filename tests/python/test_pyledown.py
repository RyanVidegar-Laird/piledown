import pandas as pd
import pyarrow as pa
import pytest

from pyledown import LibFragmentType, PileParams, Strand


def test_single_region_matches_golden(test_bam, golden_dir):
    """Path 1: single region= with golden fixture comparison."""
    reader = PileParams(
        input_bam=test_bam,
        lib_fragment_type=LibFragmentType.Isr,
        region="chr1:14900-15200",
        name="golden_test",
        strand=Strand.Reverse,
    ).generate()

    actual = reader.read_all().to_pandas()

    golden = pd.read_csv(golden_dir / "chr1_14900-15200_isr_reverse.tsv", sep="\t")

    # Compare only shared columns; cast golden int64 → UInt64 to match Arrow output
    for col in ("pos", "up", "down"):
        pd.testing.assert_series_equal(
            actual[col].reset_index(drop=True),
            golden[col].astype("UInt64").reset_index(drop=True),
            check_names=False,
            check_dtype=False,
        )
