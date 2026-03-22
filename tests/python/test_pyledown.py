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


def test_region_strings(test_bam):
    """Path 2: regions= with parallel names/strands lists."""
    reader = PileParams(
        input_bam=test_bam,
        lib_fragment_type=LibFragmentType.Isr,
        regions=["chr1:17000-25000", "chr1:26000-30000"],
        names=["tes1", "tes2"],
        strands=[Strand.Forward, Strand.Reverse],
    ).generate()

    table = reader.read_all()
    df = table.to_pandas()

    assert table.column_names == ["name", "seq", "strand", "pos", "up", "down"]
    assert len(df) == (25000 - 17000 + 1) + (30000 - 26000 + 1)  # regions are inclusive
    assert set(df["name"]) == {"tes1", "tes2"}


def test_decomposed_vectors(test_bam):
    """Path 3: seqs=/starts=/ends= with decomposed vectors."""
    reader = PileParams(
        input_bam=test_bam,
        lib_fragment_type=LibFragmentType.Isr,
        seqs=["chr1", "chr1"],
        starts=[17000, 26000],
        ends=[25000, 30000],
        names=["tes1", "tes2"],
        strands=[Strand.Forward, Strand.Reverse],
    ).generate()

    table = reader.read_all()
    df = table.to_pandas()

    assert table.column_names == ["name", "seq", "strand", "pos", "up", "down"]
    assert len(df) == (25000 - 17000 + 1) + (30000 - 26000 + 1)  # regions are inclusive
    assert set(df["name"]) == {"tes1", "tes2"}


def test_dataframe_input(test_bam):
    """Path 4: regions_df= with pandas DataFrame."""
    df_in = pd.DataFrame({
        "seq": ["chr1", "chr1"],
        "start": [17000, 26000],
        "end": [25000, 30000],
        "name": ["tes1", "tes2"],
        "strand": ["+", "-"],
    })

    reader = PileParams(
        input_bam=test_bam,
        lib_fragment_type=LibFragmentType.Isr,
        regions_df=df_in,
    ).generate()

    table = reader.read_all()
    df = table.to_pandas()

    assert table.column_names == ["name", "seq", "strand", "pos", "up", "down"]
    assert len(df) == (25000 - 17000 + 1) + (30000 - 26000 + 1)  # regions are inclusive
    assert set(df["name"]) == {"tes1", "tes2"}


def test_regions_file(test_bam, regions_file):
    """Path 5: regions_file= with TSV path."""
    reader = PileParams(
        input_bam=test_bam,
        lib_fragment_type=LibFragmentType.Isr,
        regions_file=regions_file,
    ).generate()

    table = reader.read_all()

    assert table.column_names == ["name", "seq", "strand", "pos", "up", "down"]
    assert table.num_rows > 0


def test_output_schema(test_bam):
    """Verify Arrow schema column names and types."""
    reader = PileParams(
        input_bam=test_bam,
        lib_fragment_type=LibFragmentType.Isr,
        region="chr1:14900-15200",
        name="schema_test",
        strand=Strand.Reverse,
    ).generate()

    schema = reader.schema

    assert schema.names == ["name", "seq", "strand", "pos", "up", "down"]
    assert schema.field("name").type == pa.string()
    assert schema.field("seq").type == pa.string()
    assert isinstance(schema.field("strand").type, pa.DictionaryType)
    assert schema.field("pos").type == pa.uint64()
    assert schema.field("up").type == pa.uint64()
    assert schema.field("down").type == pa.uint64()


def test_isf_differs_from_isr(test_bam):
    """ISF and ISR on same region produce different coverage values."""
    kwargs = dict(
        input_bam=test_bam,
        region="chr1:14900-15200",
        name="cmp",
        strand=Strand.Reverse,
    )

    isr_df = (
        PileParams(lib_fragment_type=LibFragmentType.Isr, **kwargs)
        .generate()
        .read_all()
        .to_pandas()
    )
    isf_df = (
        PileParams(lib_fragment_type=LibFragmentType.Isf, **kwargs)
        .generate()
        .read_all()
        .to_pandas()
    )

    assert len(isr_df) > 0
    assert len(isf_df) > 0
    # At least one position should differ in up or down
    assert not (isr_df["up"].values == isf_df["up"].values).all() or not (
        isr_df["down"].values == isf_df["down"].values
    ).all()
