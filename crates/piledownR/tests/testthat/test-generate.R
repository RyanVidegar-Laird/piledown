# ---- Existing integration tests, updated for new API ----

test_that("generate returns Arrow RecordBatchReader with correct schema", {
  bam_path <- system.file("testdata", "SRR21778056-sorted-subsample.bam", package = "piledownR")
  skip_if(bam_path == "", message = "Test BAM not found in inst/testdata")
  params <- pile_params(
    input_bam = bam_path, lib_fragment_type = "isr",
    region = "chr1:14900-15200", name = "region", strand = "-"
  )
  reader <- generate(params)
  expect_s3_class(reader, "RecordBatchReader")
  table <- reader$read_table()
  expected_cols <- c("name", "seq", "strand", "pos", "up", "down")
  expect_true(all(expected_cols %in% names(table)))
  expect_gt(nrow(table), 0)
})

test_that("generate works with multiple regions via regions_file", {
  bam_path <- system.file("testdata", "SRR21778056-sorted-subsample.bam", package = "piledownR")
  skip_if(bam_path == "", message = "Test BAM not found in inst/testdata")
  regions_path <- system.file("testdata", "regions.tsv", package = "piledownR")
  skip_if(regions_path == "", message = "Test regions file not found")
  params <- pile_params(input_bam = bam_path, lib_fragment_type = "isr", regions_file = regions_path)
  reader <- generate(params)
  table <- reader$read_table()
  expect_gt(nrow(table), 0)
})

test_that("pile_params rejects invalid arguments", {
  expect_error(pile_params(input_bam = "test.bam", lib_fragment_type = "invalid",
    region = "chr1:100-200", name = "r1", strand = "+"))
  expect_error(pile_params(input_bam = "test.bam", lib_fragment_type = "isr"))
})

# ---- New tests for per-region strand API ----

test_that("pile_params works with single region", {
  params <- pile_params(input_bam = "test.bam", lib_fragment_type = "isr",
    region = "chr1:100-200", name = "gene1", strand = "+")
  expect_s3_class(params, "PileParams")
})

test_that("pile_params works with parallel region strings", {
  params <- pile_params(input_bam = "test.bam", lib_fragment_type = "isr",
    regions = c("chr1:100-200", "chr2:300-400"),
    names = c("g1", "g2"), strands = c("+", "-"))
  expect_s3_class(params, "PileParams")
})

test_that("pile_params works with decomposed vectors", {
  params <- pile_params(input_bam = "test.bam", lib_fragment_type = "isr",
    seqs = c("chr1", "chr2"), starts = c(100, 300), ends = c(200, 400),
    names = c("g1", "g2"), strands = c("+", "-"))
  expect_s3_class(params, "PileParams")
})

test_that("pile_params works with data.frame", {
  df <- data.frame(seq = c("chr1", "chr2"), start = c(100, 300), end = c(200, 400),
    name = c("g1", "g2"), strand = c("+", "-"), stringsAsFactors = FALSE)
  params <- pile_params(input_bam = "test.bam", lib_fragment_type = "isr", regions_df = df)
  expect_s3_class(params, "PileParams")
})

test_that("pile_params works with tibble", {
  skip_if_not_installed("tibble")
  tbl <- tibble::tibble(seq = c("chr1", "chr2"), start = c(100, 300), end = c(200, 400),
    name = c("g1", "g2"), strand = c("+", "-"))
  params <- pile_params(input_bam = "test.bam", lib_fragment_type = "isr", regions_df = tbl)
  expect_s3_class(params, "PileParams")
})

test_that("pile_params rejects multiple region sources", {
  expect_error(pile_params(input_bam = "test.bam", lib_fragment_type = "isr",
    region = "chr1:100-200", name = "g1", strand = "+",
    regions = c("chr1:100-200"), names = c("g1"), strands = c("+")))
})

test_that("pile_params rejects missing companions", {
  expect_error(pile_params(input_bam = "test.bam", lib_fragment_type = "isr",
    regions = c("chr1:100-200")))
})

test_that("pile_params rejects length mismatch", {
  expect_error(pile_params(input_bam = "test.bam", lib_fragment_type = "isr",
    regions = c("chr1:100-200"), names = c("g1", "g2"), strands = c("+")))
})

test_that("pile_params rejects missing DataFrame columns", {
  df <- data.frame(seq = "chr1", start = 100, end = 200)
  expect_error(pile_params(input_bam = "test.bam", lib_fragment_type = "isr", regions_df = df))
})

test_that("pile_params works with TSV file path", {
  tmp <- tempfile(fileext = ".tsv")
  writeLines(c("seq\tstart\tend\tname\tstrand", "chr1\t100\t200\tg1\t+", "chr2\t300\t400\tg2\t-"), tmp)
  params <- pile_params(input_bam = "test.bam", lib_fragment_type = "isr", regions_file = tmp)
  expect_s3_class(params, "PileParams")
  unlink(tmp)
})

test_that("generate works with per-region strand", {
  bam_path <- system.file("testdata", "SRR21778056-sorted-subsample.bam", package = "piledownR")
  skip_if(bam_path == "", message = "Test BAM not found in inst/testdata")
  params <- pile_params(input_bam = bam_path, lib_fragment_type = "isr",
    regions = c("chr1:14900-15000", "chr1:15000-15200"),
    names = c("region_a", "region_b"), strands = c("-", "-"))
  reader <- generate(params)
  table <- reader$read_table()
  expect_gt(nrow(table), 0)
  region_names <- unique(as.character(table$name))
  expect_true("region_a" %in% region_names)
  expect_true("region_b" %in% region_names)
})
