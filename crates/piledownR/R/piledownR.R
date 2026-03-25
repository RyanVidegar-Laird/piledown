#' @useDynLib piledownR, .registration = TRUE
NULL

#' Create a new PileParams for coverage computation.
#'
#' Exactly one region source must be provided: \code{region} (single),
#' \code{regions} (parallel vectors), \code{seqs} (decomposed vectors),
#' \code{regions_df} (data.frame/tibble), or \code{regions_file} (TSV path).
#'
#' @param input_bam Path to indexed BAM file.
#' @param lib_fragment_type One of "isr", "isf".
#' @param region Optional single region string (e.g. "chr1:100-200").
#' @param name Region name (required with \code{region}).
#' @param strand Strand string (required with \code{region}).
#' @param regions Optional character vector of region strings.
#' @param names Character vector of region names.
#' @param strands Character vector of strand strings.
#' @param seqs Optional character vector of sequence names.
#' @param starts Numeric vector of start positions.
#' @param ends Numeric vector of end positions.
#' @param regions_df Optional data.frame or tibble with columns: seq, start, end, name, strand.
#' @param regions_file Optional path to TSV regions file.
#' @param exclude_flags Optional SAM flags to exclude (integer 0-65535).
#' @param index_path Optional explicit path to BAM index (.bai).
#' @param concurrency Number of concurrent region processors (default 4).
#' @param chunk_size Optional chunk size for splitting large regions.
#' @param anchor_length Minimum matched bases flanking a junction (default NULL = 0, no filtering).
#' @return A PileParams object.
#' @export
pile_params <- function(
    input_bam,
    lib_fragment_type,
    region = NULL, name = NULL, strand = NULL,
    regions = NULL, names = NULL, strands = NULL,
    seqs = NULL, starts = NULL, ends = NULL,
    regions_df = NULL, regions_file = NULL,
    exclude_flags = NULL, index_path = NULL,
    concurrency = NULL, chunk_size = NULL,
    anchor_length = NULL) {

  # Phase 1: detect active group
  groups <- c(
    region = !is.null(region),
    regions = !is.null(regions),
    seqs = !is.null(seqs),
    regions_df = !is.null(regions_df),
    regions_file = !is.null(regions_file)
  )
  active <- sum(groups)
  if (active != 1) {
    stop("provide exactly one of: region, regions, seqs, regions_df, regions_file")
  }

  # Phase 2: validate and dispatch
  if (groups["region"]) {
    if (is.null(name) || is.null(strand)) {
      stop("'region' requires 'name' and 'strand'")
    }
    PileParams$new(
      input_bam, lib_fragment_type,
      region = region, name = name, strand = strand,
      regions = NULL, region_names = NULL, region_strands = NULL,
      seqs = NULL, starts = NULL, ends = NULL,
      regions_file = NULL,
      exclude_flags = exclude_flags, index_path = index_path,
      concurrency = concurrency, chunk_size = chunk_size,
      anchor_length = anchor_length
    )
  } else if (groups["regions"]) {
    if (is.null(names) || is.null(strands)) {
      stop("'regions' requires 'names' and 'strands' of equal length")
    }
    if (length(regions) != length(names) || length(regions) != length(strands)) {
      stop(sprintf(
        "'regions' (%d), 'names' (%d), 'strands' (%d) must all be the same length",
        length(regions), length(names), length(strands)
      ))
    }
    PileParams$new(
      input_bam, lib_fragment_type,
      region = NULL, name = NULL, strand = NULL,
      regions = regions, region_names = names, region_strands = strands,
      seqs = NULL, starts = NULL, ends = NULL,
      regions_file = NULL,
      exclude_flags = exclude_flags, index_path = index_path,
      concurrency = concurrency, chunk_size = chunk_size,
      anchor_length = anchor_length
    )
  } else if (groups["seqs"]) {
    if (is.null(starts) || is.null(ends) || is.null(names) || is.null(strands)) {
      stop("'seqs' requires 'starts', 'ends', 'names', and 'strands'")
    }
    lens <- c(length(seqs), length(starts), length(ends), length(names), length(strands))
    if (length(unique(lens)) != 1) {
      stop(sprintf(
        "'seqs' (%d), 'starts' (%d), 'ends' (%d), 'names' (%d), 'strands' (%d) must all be the same length",
        lens[1], lens[2], lens[3], lens[4], lens[5]
      ))
    }
    PileParams$new(
      input_bam, lib_fragment_type,
      region = NULL, name = NULL, strand = NULL,
      regions = NULL, region_names = names, region_strands = strands,
      seqs = seqs, starts = as.numeric(starts), ends = as.numeric(ends),
      regions_file = NULL,
      exclude_flags = exclude_flags, index_path = index_path,
      concurrency = concurrency, chunk_size = chunk_size,
      anchor_length = anchor_length
    )
  } else if (groups["regions_df"]) {
    if (!is.data.frame(regions_df)) {
      stop("'regions_df' must be a data.frame or tibble")
    }
    required_cols <- c("seq", "start", "end", "name", "strand")
    missing <- setdiff(required_cols, colnames(regions_df))
    if (length(missing) > 0) {
      stop(sprintf(
        "regions_df must have columns: seq, start, end, name, strand (missing: %s)",
        paste(missing, collapse = ", ")
      ))
    }
    PileParams$new(
      input_bam, lib_fragment_type,
      region = NULL, name = NULL, strand = NULL,
      regions = NULL, region_names = as.character(regions_df$name),
      region_strands = as.character(regions_df$strand),
      seqs = as.character(regions_df$seq),
      starts = as.numeric(regions_df$start),
      ends = as.numeric(regions_df$end),
      regions_file = NULL,
      exclude_flags = exclude_flags, index_path = index_path,
      concurrency = concurrency, chunk_size = chunk_size,
      anchor_length = anchor_length
    )
  } else if (groups["regions_file"]) {
    PileParams$new(
      input_bam, lib_fragment_type,
      region = NULL, name = NULL, strand = NULL,
      regions = NULL, region_names = NULL, region_strands = NULL,
      seqs = NULL, starts = NULL, ends = NULL,
      regions_file = regions_file,
      exclude_flags = exclude_flags, index_path = index_path,
      concurrency = concurrency, chunk_size = chunk_size,
      anchor_length = anchor_length
    )
  }
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

#' Create a new JunctionParams for junction counting.
#'
#' Exactly one junction source must be provided: \code{seqs} (decomposed vectors),
#' \code{junctions_df} (data.frame/tibble), or \code{junctions_file} (TSV path).
#'
#' @param input_bam Path to indexed BAM file.
#' @param lib_fragment_type One of "isr", "isf".
#' @param seqs Optional character vector of sequence names.
#' @param starts Numeric vector of junction start positions.
#' @param ends Numeric vector of junction end positions.
#' @param names Character vector of junction names.
#' @param strands Character vector of strand strings.
#' @param junctions_df Optional data.frame or tibble with columns: seq, start, end, name, strand.
#' @param junctions_file Optional path to TSV junctions file.
#' @param exclude_flags Optional SAM flags to exclude (integer 0-65535).
#' @param index_path Optional explicit path to BAM index (.bai).
#' @param concurrency Number of concurrent processors (default 4).
#' @param anchor_length Minimum flanking match bases (default NULL = 0).
#' @return A JunctionParams object.
#' @export
junction_params <- function(
    input_bam,
    lib_fragment_type,
    seqs = NULL, starts = NULL, ends = NULL,
    names = NULL, strands = NULL,
    junctions_df = NULL, junctions_file = NULL,
    exclude_flags = NULL, index_path = NULL,
    concurrency = NULL, anchor_length = NULL) {

  groups <- c(
    seqs = !is.null(seqs),
    junctions_df = !is.null(junctions_df),
    junctions_file = !is.null(junctions_file)
  )
  active <- sum(groups)
  if (active != 1) {
    stop("provide exactly one of: seqs, junctions_df, junctions_file")
  }

  if (groups["seqs"]) {
    if (is.null(starts) || is.null(ends) || is.null(names) || is.null(strands)) {
      stop("'seqs' requires 'starts', 'ends', 'names', and 'strands'")
    }
    lens <- c(length(seqs), length(starts), length(ends), length(names), length(strands))
    if (length(unique(lens)) != 1) {
      stop(sprintf(
        "'seqs' (%d), 'starts' (%d), 'ends' (%d), 'names' (%d), 'strands' (%d) must all be the same length",
        lens[1], lens[2], lens[3], lens[4], lens[5]
      ))
    }
    JunctionParams$new(
      input_bam, lib_fragment_type,
      seqs = seqs, starts = as.numeric(starts), ends = as.numeric(ends),
      region_names = names, region_strands = strands,
      junctions_file = NULL,
      exclude_flags = exclude_flags, index_path = index_path,
      concurrency = concurrency, anchor_length = anchor_length
    )
  } else if (groups["junctions_df"]) {
    if (!is.data.frame(junctions_df)) {
      stop("'junctions_df' must be a data.frame or tibble")
    }
    required_cols <- c("seq", "start", "end", "name", "strand")
    missing <- setdiff(required_cols, colnames(junctions_df))
    if (length(missing) > 0) {
      stop(sprintf(
        "junctions_df must have columns: seq, start, end, name, strand (missing: %s)",
        paste(missing, collapse = ", ")
      ))
    }
    JunctionParams$new(
      input_bam, lib_fragment_type,
      seqs = as.character(junctions_df$seq),
      starts = as.numeric(junctions_df$start),
      ends = as.numeric(junctions_df$end),
      region_names = as.character(junctions_df$name),
      region_strands = as.character(junctions_df$strand),
      junctions_file = NULL,
      exclude_flags = exclude_flags, index_path = index_path,
      concurrency = concurrency, anchor_length = anchor_length
    )
  } else if (groups["junctions_file"]) {
    JunctionParams$new(
      input_bam, lib_fragment_type,
      seqs = NULL, starts = NULL, ends = NULL,
      region_names = NULL, region_strands = NULL,
      junctions_file = junctions_file,
      exclude_flags = exclude_flags, index_path = index_path,
      concurrency = concurrency, anchor_length = anchor_length
    )
  }
}

#' Count reads matching each junction.
#'
#' Runs the junction engine on the BAM file and junctions configured in the
#' given JunctionParams object. Returns an Arrow RecordBatchReader with columns:
#' name, seq, strand, start, end, count.
#'
#' @param params A JunctionParams object created via \code{junction_params()}.
#' @return An \code{arrow::RecordBatchReader}.
#' @export
generate_stream <- function(params) {
  stream <- params$generate_stream()
  arrow::as_record_batch_reader(stream)
}
