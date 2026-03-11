# Golden Test Fixtures

Generated from `SRR21778056-sorted-subsample.bam` using the pre-rearchitecture `pldn` binary.
These serve as regression baselines — the rearchitected code must produce identical coverage values.

## Schema

The old `pldn` produces 4-column TSV: `seq, pos, strand, up, down` (no `name` column).
The new code produces 6-column TSV: `name, seq, strand, pos, up, down`.
Integration tests should compare coverage values (up/down per position), not raw TSV strings.

## Samtools cross-validation

`*_samtools_depth_all.tsv` contains `samtools depth -a -g 0x704` output (all positions,
no default flag filtering). The `up` column from pldn with `-s either` should match the
samtools depth column exactly — this was verified at fixture generation time.

Samtools flags: `-g 0x704` removes the default filter-out list (UNMAP, SECONDARY, QCFAIL, DUP)
so all reads are counted, matching pldn's behavior when no `-e` exclude flags are set.

## Regenerating

Only regenerate if the test BAM changes or a bug is found in the original output.
