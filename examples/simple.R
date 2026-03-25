library(piledownR)
library(arrow)
library(ggplot2)
library(tidyr)
library(dplyr)

input_bam <- file.path("tests", "data", "SRR21778056-sorted-subsample.bam")

# --- Coverage ---

regions <- tibble::tibble(
  seq = c("chr1", "chr1"),
  start = c(14000, 20000),
  end = c(20000, 25000),
  name = c("region_a", "region_b"),
  strand = c("-", "-")
)

params <- pile_params(
  input_bam = input_bam,
  lib_fragment_type = "isr",
  regions_df = regions
)

reader <- generate(params)
df <- as_tibble(reader$read_table())
df$down <- -df$down

plot_df <- df[df$name == "region_a", ]
long <- pivot_longer(plot_df, cols = c("up", "down"), names_to = "direction", values_to = "count")

ggplot(long, aes(x = pos, y = count, colour = direction)) +
  geom_line() +
  labs(x = "Position", y = "Coverage", title = "region_a: chr1:14000-20000 (reverse strand)") +
  theme_minimal()

# --- Junction counting ---

jp <- junction_params(
  input_bam = input_bam,
  lib_fragment_type = "isr",
  seqs = c("chr1", "chr1"),
  starts = c(153990803, 23694792),
  ends = c(153991114, 23695797),
  region_names = c("junc_a", "junc_b"),
  region_strands = c("+", "-")
)

junc_reader <- generate_stream(jp)
junc_df <- as_tibble(junc_reader$read_table())
print(junc_df)
