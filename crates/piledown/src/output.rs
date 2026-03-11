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
        let region = PileRegion::new("chr1".into(), 100, 102, "test".into(), Strand::Forward);
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
        let region = PileRegion::new("chr1".into(), 100, 104, "test".into(), Strand::Forward);
        let map = CoverageMap::new(100, 104);
        let batch = to_record_batch(&region, &map).unwrap();
        assert_eq!(batch.num_rows(), 5);
    }

    #[test]
    fn record_batch_reflects_coverage_values() {
        let region = PileRegion::new("chr1".into(), 100, 102, "test".into(), Strand::Forward);
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
}
