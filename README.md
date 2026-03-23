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
Like a pileup, but also down...

Per-base coverage from RNA-seq BAMs -- matches *and* skips.

Standard coverage tools like `samtools depth` count how many reads overlap each position, but they don't distinguish between bases that were **matched** (M/=/X CIGAR ops) and bases that were **skipped** (N ops, i.e. spliced-out introns). Piledown counts both separately, giving you `up` (match) and `down` (skip) counts at every position in a region. This is useful for splice-aware QC, intron retention analysis, and junction coverage profiling.

Other features:
- **Strand-aware** -- filter reads by inferred transcript strand using ISR/ISF library protocols (same conventions as Salmon)
- **Fast** -- async BAM I/O via noodles with configurable concurrency for multi-region queries
- **Cross-language** -- Rust library, CLI (`pldn`), Python bindings (`pyledown`), and R bindings (`piledownR`) all from the one codebase
- **Flexible output** -- TSV, Arrow IPC, or Parquet
  - Zero-copy transfers into data frames in Python and R.

## Installation

Statically linked `pldn` binaries are available on the [releases page](https://github.com/RyanVidegar-Laird/piledown/releases). The Python and R bindings are available via the Nix flake.

Add Piledown to your `flake.nix`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/25.11";
    piledown.url = "github:RyanVidegar-Laird/piledown";
    piledown.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { nixpkgs, piledown, ... }:
    let
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      piledownPkgs = piledown.packages.x86_64-linux;
    in {
      # CLI
      packages.x86_64-linux.default = piledownPkgs.pldn;

      # Python environment with pyledown
      devShells.x86_64-linux.default = pkgs.mkShell {
        packages = [
          (pkgs.python3.withPackages (ps: [ piledownPkgs.pyledown ps.pandas ]))
        ];
      };

      # R environment with piledownR
      devShells.x86_64-linux.r = pkgs.mkShell {
        packages = [
          (pkgs.rWrapper.override {
            packages = [ piledownPkgs.piledownR pkgs.rPackages.dplyr ];
          })
        ];
      };
    };
}
```

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

### Regions file format

When using `--regions-file` (CLI), `regions_file` (Python/R), the file is a tab-separated file with a header row:

```
seq	start	end	name	strand
chr1	17000	25000	gene_a	+
chr1	26000	30000	gene_b	-
```

Required columns: `seq`, `start`, `end`, `name`, `strand`. An optional `anchor` column sets per-region anchor lengths (see below).

### Anchor length

`anchor_length` filters out reads where fewer than N bases are matched on either side of a splice. This removes low-confidence junction evidence from both `up` and `down` counts. Defaults to 0 (no filtering). Can also be set per-region via the `anchor` column in a regions file.

Piledown does not identify junctions -- it only counts matches and skips per position. To get anchor-filtered counts at a specific junction, pass a 1 bp region at the junction position.

### Batch regions to Parquet, then query with DuckDB

Parquet requires random access so it can't be piped -- write to a file, then query:

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

### Python

```python
import pyledown
import pyarrow

params = pyledown.PileParams(
    input_bam="sample.bam",
    region="chr1:14900-15200",
    name="my_region",
    strand=pyledown.Strand.Reverse,
    lib_fragment_type=pyledown.LibFragmentType.Isr,
)

batch = params.generate()  # returns a pyarrow.RecordBatch
df = batch.to_pandas()

print(df[df["up"] > 0].head())
```

### R

```r
library(piledownR)
library(dplyr)

params <- pile_params(
  input_bam = "sample.bam",
  region = "chr1:14900-15200",
  name = "my_region",
  strand = "reverse",
  lib_fragment_type = "isr"
)

reader <- generate(params)  # returns arrow::RecordBatchReader
df <- as_tibble(reader$read_table())

df |> filter(up > 0) |> head()
```
