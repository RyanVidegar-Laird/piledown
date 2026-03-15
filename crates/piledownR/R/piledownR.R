#' @useDynLib piledownR, .registration = TRUE
NULL

#' Create a new PileParams for coverage computation.
#'
#' @param input_bam Path to indexed BAM file.
#' @param strand One of "forward", "reverse", "either".
#' @param lib_fragment_type One of "isr", "isf".
#' @param region Optional region string (e.g. "chr1:100-200").
#' @param regions Optional character vector of region strings.
#' @param regions_file Optional path to TSV regions file.
#' @param exclude_flags Optional SAM flags to exclude (integer 0-65535).
#' @param index_path Optional explicit path to BAM index (.bai).
#' @param concurrency Number of concurrent region processors (default 4).
#' @param chunk_size Optional chunk size for splitting large regions.
#' @return A PileParams object.
#' @export
pile_params <- function(
    input_bam,
    strand,
    lib_fragment_type,
    region = NULL,
    regions = NULL,
    regions_file = NULL,
    exclude_flags = NULL,
    index_path = NULL,
    concurrency = NULL,
    chunk_size = NULL) {
  PileParams$new(
    input_bam, strand, lib_fragment_type,
    region, regions, regions_file,
    exclude_flags, index_path, concurrency, chunk_size
  )
}

#' Generate per-base coverage for configured regions.
#'
#' Runs the piledown engine on the BAM file and regions configured in the
#' given PileParams object. Returns an Arrow RecordBatchReader with columns:
#' name, seq, strand, pos, up, down.
#'
#' @param params A PileParams object created via \code{pile_params()}.
#' @return An \code{arrow::RecordBatchReader}.
#' @export
generate <- function(params) {
  stream <- params$generate_stream()
  arrow::as_record_batch_reader(stream)
}
