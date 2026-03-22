from pathlib import Path

import pytest

# tests/python/conftest.py → project root is two levels up
ROOT = Path(__file__).resolve().parent.parent.parent


@pytest.fixture
def test_bam():
    path = ROOT / "tests" / "data" / "SRR21778056-sorted-subsample.bam"
    if not path.exists():
        pytest.skip(f"Test BAM not found: {path}")
    return str(path)


@pytest.fixture
def regions_file():
    path = ROOT / "tests" / "data" / "regions.tsv"
    if not path.exists():
        pytest.skip(f"Regions file not found: {path}")
    return str(path)


@pytest.fixture
def golden_dir():
    path = ROOT / "tests" / "golden"
    if not path.exists():
        pytest.skip(f"Golden dir not found: {path}")
    return path
