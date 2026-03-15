test_that("generate returns Arrow RecordBatchReader with correct schema", {
  bam_path <- system.file(
    "testdata", "SRR21778056-sorted-subsample.bam",
    package = "piledownR"
  )
  skip_if(bam_path == "", message = "Test BAM not found in inst/testdata")

  params <- pile_params(
    input_bam = bam_path,
    strand = "reverse",
    lib_fragment_type = "isr",
    region = "chr1:14900-15200"
  )
  reader <- generate(params)
  expect_s3_class(reader, "RecordBatchReader")

  table <- reader$read_table()
  expected_cols <- c("name", "seq", "strand", "pos", "up", "down")
  expect_true(all(expected_cols %in% names(table)))
  expect_gt(nrow(table), 0)
})

test_that("generate works with multiple regions", {
  bam_path <- system.file(
    "testdata", "SRR21778056-sorted-subsample.bam",
    package = "piledownR"
  )
  skip_if(bam_path == "", message = "Test BAM not found in inst/testdata")

  params <- pile_params(
    input_bam = bam_path,
    strand = "reverse",
    lib_fragment_type = "isr",
    regions = c("chr1:14900-15000", "chr1:15000-15200")
  )
  reader <- generate(params)
  table <- reader$read_table()
  expect_gt(nrow(table), 0)
})

test_that("pile_params rejects invalid arguments", {
  # extendr converts panics to R errors with "User function panicked" message
  expect_error(
    pile_params(
      input_bam = "test.bam",
      strand = "invalid",
      lib_fragment_type = "isr",
      region = "chr1:100-200"
    )
  )

  expect_error(
    pile_params(
      input_bam = "test.bam",
      strand = "forward",
      lib_fragment_type = "invalid",
      region = "chr1:100-200"
    )
  )

  expect_error(
    pile_params(
      input_bam = "test.bam",
      strand = "forward",
      lib_fragment_type = "isr"
    )
  )
})
