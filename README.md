```
           ____   _  _           ---      -......----
          / __ \ (_)/ /___      --- --.....--- ----
         / /_/ // // // _ \   ----  --.....-- -----
        / ____// // //  __/   --[\\\\\]------[\\\\\]--
       / /    /_//_/ \___/       __               _
  ____/ /____  _      __ ____   /  \_        _   / \
 / __  // __ \| | /| / // __ \ /     |      / \_/   \
/ /_/ // /_/ /| |/ |/ // / / //______|_____/_________|
\__,_/ \____/ |__/|__//_/ /_/        \    /_____/
                                      \__/
```

# Piledown

Per-base coverage from RNA-seq BAMs -- matches *and* skips.

Standard coverage tools like `samtools depth` count how many reads overlap each position, but they don't distinguish between bases that were **matched** (M/=/X CIGAR ops) and bases that were **skipped** (N ops, i.e. spliced-out introns). Piledown counts both separately, giving you `up` (match) and `down` (skip) counts at every position in a region. This is useful for splice-aware QC, intron retention analysis, and junction coverage profiling.

Piledown is also:

- **Strand-aware** -- filter reads by inferred transcript strand using ISR/ISF library protocols (same conventions as Salmon)
- **Fast** -- async BAM I/O via noodles with configurable concurrency for multi-region queries
- **Cross-language** -- Rust library, CLI (`pldn`), and Python bindings (`pyledown`) all from the same codebase
- **Flexible output** -- TSV, Arrow IPC, or Parquet

## Getting Started

### CLI

The CLI binary is `pldn`. Single region to TSV:

```bash
pldn sample.bam \
  --region "chr1:14900-15200" \
  --strand reverse \
  --lib-fragment-type isr
```

Output:

```
name	seq	strand	pos	up	down
region	chr1	-	14900	0	0
region	chr1	-	14901	0	0
region	chr1	-	14902	1	0
...
```

### Pipe TSV into DuckDB

TSV output can be piped directly into DuckDB for ad-hoc queries:

```bash
pldn sample.bam \
  --region "chr1:14900-15200" \
  --strand reverse \
  --lib-fragment-type isr \
| duckdb -c "
    SELECT pos, up, down
    FROM read_csv('/dev/stdin', delim='\t', header=true)
    WHERE up > 0 AND down > 0
  "
```

### Batch regions to Parquet, then query with DuckDB

For multi-region batch jobs, Parquet is more efficient. Parquet requires random access so it can't be piped -- write to a file, then query:

```bash
pldn sample.bam \
  --regions-file regions.tsv \
  --lib-fragment-type isr \
  --output-format parquet \
  --concurrency 8 \
  > coverage.parquet

duckdb -c "
  SELECT
    name,
    quantile_cont(up, [0.25, 0.5, 0.75]) AS up_quartiles,
    quantile_cont(down, [0.25, 0.5, 0.75]) AS down_quartiles
  FROM 'coverage.parquet'
  GROUP BY name
"
```

The regions TSV should have columns: `seq`, `start`, `end`, `name`, `strand` (where strand is `+`, `-`, or `.`).

### Python

```python
import pyledown
import pyarrow

params = pyledown.PileParams(
    input_bam="sample.bam",
    region="chr1:14900-15200",
    strand=pyledown.Strand.Reverse,
    lib_fragment_type=pyledown.LibFragmentType.Isr,
)

batch = params.generate()  # returns a pyarrow.RecordBatch
df = batch.to_pandas()

print(df[df["up"] > 0].head())
```
