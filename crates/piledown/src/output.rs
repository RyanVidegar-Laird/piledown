use std::sync::Arc;

use anyhow::{anyhow, Result};
use arrow::array::{
    ArrayAccessor, GenericStringBuilder, RecordBatch, StringDictionaryBuilder, UInt64Builder,
};
use arrow::datatypes::{DataType, Field, Int8Type, Schema};
use parquet::arrow::ArrowWriter;
use parquet::basic::Encoding;
use parquet::file::properties::WriterProperties;
use parquet::schema::types::ColumnPath;

use crate::coverage::CoverageMap;
use crate::region::PileRegion;
use crate::types::OutputFormat;

pub fn to_record_batch(region: &PileRegion, map: &CoverageMap) -> Result<RecordBatch> {
    let schema = Schema::new(vec![
        Field::new("name", DataType::Utf8, false),
        Field::new("seq", DataType::Utf8, false),
        Field::new_dictionary("strand", DataType::Int8, DataType::Utf8, false),
        Field::new("pos", DataType::UInt64, false),
        Field::new("up", DataType::UInt64, false),
        Field::new("down", DataType::UInt64, false),
    ]);

    let n = map.len();
    let mut name = GenericStringBuilder::<i32>::new();
    let mut seq = GenericStringBuilder::<i32>::new();
    let mut strand = StringDictionaryBuilder::<Int8Type>::with_capacity(3, n, n * 8);
    let mut pos = UInt64Builder::with_capacity(n);
    let mut up = UInt64Builder::with_capacity(n);
    let mut down = UInt64Builder::with_capacity(n);

    for (i, cov) in map.counts.iter().enumerate() {
        name.append_value(&region.name);
        seq.append_value(&region.seq);
        strand.append_value(region.strand.as_ref());
        pos.append_value(map.start + i as u64);
        up.append_value(cov.up);
        down.append_value(cov.down);
    }

    let batch = RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(name.finish()),
            Arc::new(seq.finish()),
            Arc::new(strand.finish()),
            Arc::new(pos.finish()),
            Arc::new(up.finish()),
            Arc::new(down.finish()),
        ],
    )?;
    Ok(batch)
}

/// Write a RecordBatch to the given writer in the specified format.
/// `write_header` controls whether TSV output includes a header row.
/// Set to `true` for the first batch, `false` for subsequent batches in streaming mode.
/// Ignored for Arrow/Parquet formats.
pub fn write_output(
    batch: &RecordBatch,
    format: OutputFormat,
    writer: impl std::io::Write + Send,
    write_header: bool,
) -> Result<()> {
    match format {
        OutputFormat::Tsv => {
            let mut w = csv::WriterBuilder::new()
                .delimiter(b'\t')
                .from_writer(writer);
            if write_header {
                w.write_record(["name", "seq", "strand", "pos", "up", "down"])?;
            }

            let name_col = batch
                .column(0)
                .as_any()
                .downcast_ref::<arrow::array::StringArray>()
                .ok_or_else(|| anyhow!("expected StringArray for 'name' column"))?;
            let seq_col = batch
                .column(1)
                .as_any()
                .downcast_ref::<arrow::array::StringArray>()
                .ok_or_else(|| anyhow!("expected StringArray for 'seq' column"))?;
            let strand_arr = batch
                .column(2)
                .as_any()
                .downcast_ref::<arrow::array::DictionaryArray<Int8Type>>()
                .ok_or_else(|| anyhow!("expected DictionaryArray for 'strand' column"))?;
            let strand_col = strand_arr
                .downcast_dict::<arrow::array::StringArray>()
                .ok_or_else(|| anyhow!("expected StringArray values in 'strand' dictionary"))?;
            let pos_col = batch
                .column(3)
                .as_any()
                .downcast_ref::<arrow::array::UInt64Array>()
                .ok_or_else(|| anyhow!("expected UInt64Array for 'pos' column"))?;
            let up_col = batch
                .column(4)
                .as_any()
                .downcast_ref::<arrow::array::UInt64Array>()
                .ok_or_else(|| anyhow!("expected UInt64Array for 'up' column"))?;
            let down_col = batch
                .column(5)
                .as_any()
                .downcast_ref::<arrow::array::UInt64Array>()
                .ok_or_else(|| anyhow!("expected UInt64Array for 'down' column"))?;

            for i in 0..batch.num_rows() {
                w.serialize((
                    name_col.value(i),
                    seq_col.value(i),
                    strand_col.value(i),
                    pos_col.value(i),
                    up_col.value(i),
                    down_col.value(i),
                ))?;
            }
            w.flush()?;
        }
        OutputFormat::Arrow => {
            let mut w = arrow::ipc::writer::FileWriter::try_new_buffered(writer, &batch.schema())?;
            w.write(batch)?;
            w.flush()?;
            w.finish()?;
        }
        OutputFormat::Parquet => {
            let props = WriterProperties::builder()
                .set_writer_version(parquet::file::properties::WriterVersion::PARQUET_2_0)
                .set_column_encoding(ColumnPath::from("pos"), Encoding::DELTA_BINARY_PACKED)
                .set_column_encoding(ColumnPath::from("up"), Encoding::DELTA_BINARY_PACKED)
                .set_column_encoding(ColumnPath::from("down"), Encoding::DELTA_BINARY_PACKED)
                .set_compression(parquet::basic::Compression::SNAPPY)
                .build();
            let mut w = ArrowWriter::try_new(writer, batch.schema(), Some(props))?;
            w.write(batch)?;
            w.close()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coverage::CoverageMap;
    use crate::region::PileRegion;
    use crate::types::Strand;

    #[test]
    fn record_batch_has_correct_schema() {
        let region =
            PileRegion::new("chr1".into(), 100, 102, "test".into(), Strand::Forward).unwrap();
        let map = CoverageMap::new(100, 102);
        let batch = to_record_batch(&region, &map).unwrap();

        let schema = batch.schema();
        assert_eq!(schema.fields().len(), 6);
        assert_eq!(schema.field(0).name(), "name");
        assert_eq!(schema.field(1).name(), "seq");
        assert_eq!(schema.field(2).name(), "strand");
        assert_eq!(schema.field(3).name(), "pos");
        assert_eq!(schema.field(4).name(), "up");
        assert_eq!(schema.field(5).name(), "down");
    }

    #[test]
    fn record_batch_has_correct_row_count() {
        let region =
            PileRegion::new("chr1".into(), 100, 104, "test".into(), Strand::Forward).unwrap();
        let map = CoverageMap::new(100, 104);
        let batch = to_record_batch(&region, &map).unwrap();
        assert_eq!(batch.num_rows(), 5);
    }

    #[test]
    fn record_batch_reflects_coverage_values() {
        let region =
            PileRegion::new("chr1".into(), 100, 102, "test".into(), Strand::Forward).unwrap();
        let mut map = CoverageMap::new(100, 102);
        map.get_mut(101).unwrap().up = 42;
        map.get_mut(101).unwrap().down = 7;

        let batch = to_record_batch(&region, &map).unwrap();

        let up_col = batch
            .column(4)
            .as_any()
            .downcast_ref::<arrow::array::UInt64Array>()
            .unwrap();
        assert_eq!(up_col.value(1), 42);

        let down_col = batch
            .column(5)
            .as_any()
            .downcast_ref::<arrow::array::UInt64Array>()
            .unwrap();
        assert_eq!(down_col.value(1), 7);
    }

    #[test]
    fn tsv_round_trip() {
        let region = PileRegion::new(
            "chr1".into(),
            100,
            102,
            "test_region".into(),
            Strand::Forward,
        )
        .unwrap();
        let mut map = CoverageMap::new(100, 102);
        map.get_mut(100).unwrap().up = 10;
        map.get_mut(101).unwrap().up = 20;
        map.get_mut(101).unwrap().down = 5;
        map.get_mut(102).unwrap().down = 3;

        let batch = to_record_batch(&region, &map).unwrap();

        let mut buf = Vec::new();
        write_output(&batch, OutputFormat::Tsv, &mut buf, true).unwrap();

        let output = String::from_utf8(buf).unwrap();
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b'\t')
            .from_reader(output.as_bytes());

        let rows: Vec<csv::StringRecord> = reader.records().map(|r| r.unwrap()).collect();
        assert_eq!(rows.len(), 3);

        // Row 0: pos=100, up=10, down=0
        assert_eq!(&rows[0][0], "test_region");
        assert_eq!(&rows[0][1], "chr1");
        assert_eq!(&rows[0][2], "+");
        assert_eq!(&rows[0][3], "100");
        assert_eq!(&rows[0][4], "10");
        assert_eq!(&rows[0][5], "0");

        // Row 1: pos=101, up=20, down=5
        assert_eq!(&rows[1][3], "101");
        assert_eq!(&rows[1][4], "20");
        assert_eq!(&rows[1][5], "5");
    }

    #[test]
    fn tsv_header_present_when_true() {
        let region = PileRegion::new("chr1".into(), 100, 100, "t".into(), Strand::Forward).unwrap();
        let map = CoverageMap::new(100, 100);
        let batch = to_record_batch(&region, &map).unwrap();

        let mut buf = Vec::new();
        write_output(&batch, OutputFormat::Tsv, &mut buf, true).unwrap();

        let output = String::from_utf8(buf).unwrap();
        let first_line = output.lines().next().unwrap();
        assert_eq!(first_line, "name\tseq\tstrand\tpos\tup\tdown");
    }

    #[test]
    fn tsv_header_absent_when_false() {
        let region = PileRegion::new("chr1".into(), 100, 100, "t".into(), Strand::Forward).unwrap();
        let map = CoverageMap::new(100, 100);
        let batch = to_record_batch(&region, &map).unwrap();

        let mut buf = Vec::new();
        write_output(&batch, OutputFormat::Tsv, &mut buf, false).unwrap();

        let output = String::from_utf8(buf).unwrap();
        let first_line = output.lines().next().unwrap();
        // First line should be data, not header
        assert!(first_line.starts_with("t\tchr1\t+\t100\t"));
    }

    #[test]
    fn arrow_ipc_round_trip() {
        let region =
            PileRegion::new("chr1".into(), 100, 102, "test".into(), Strand::Reverse).unwrap();
        let mut map = CoverageMap::new(100, 102);
        map.get_mut(101).unwrap().up = 42;
        map.get_mut(101).unwrap().down = 7;

        let batch = to_record_batch(&region, &map).unwrap();

        let mut buf = Vec::new();
        write_output(&batch, OutputFormat::Arrow, &mut buf, true).unwrap();

        // Read back
        let cursor = std::io::Cursor::new(buf);
        let reader = arrow::ipc::reader::FileReader::try_new(cursor, None).unwrap();
        let batches: Vec<_> = reader.into_iter().map(|b| b.unwrap()).collect();
        assert_eq!(batches.len(), 1);

        let read_batch = &batches[0];
        assert_eq!(read_batch.num_rows(), 3);
        assert_eq!(read_batch.schema().fields().len(), 6);

        let up_col = read_batch
            .column(4)
            .as_any()
            .downcast_ref::<arrow::array::UInt64Array>()
            .unwrap();
        assert_eq!(up_col.value(1), 42);
    }

    #[test]
    fn parquet_round_trip() {
        let region =
            PileRegion::new("chr1".into(), 100, 102, "test".into(), Strand::Either).unwrap();
        let mut map = CoverageMap::new(100, 102);
        map.get_mut(100).unwrap().up = 99;
        map.get_mut(102).unwrap().down = 33;

        let batch = to_record_batch(&region, &map).unwrap();

        let mut buf = Vec::new();
        write_output(&batch, OutputFormat::Parquet, &mut buf, true).unwrap();

        // Read back
        let bytes = bytes::Bytes::from(buf);
        let reader =
            parquet::arrow::arrow_reader::ParquetRecordBatchReader::try_new(bytes, 1024).unwrap();
        let batches: Vec<_> = reader.into_iter().map(|b| b.unwrap()).collect();
        assert_eq!(batches.len(), 1);

        let read_batch = &batches[0];
        assert_eq!(read_batch.num_rows(), 3);

        let up_col = read_batch
            .column(4)
            .as_any()
            .downcast_ref::<arrow::array::UInt64Array>()
            .unwrap();
        assert_eq!(up_col.value(0), 99);

        let down_col = read_batch
            .column(5)
            .as_any()
            .downcast_ref::<arrow::array::UInt64Array>()
            .unwrap();
        assert_eq!(down_col.value(2), 33);
    }
}
