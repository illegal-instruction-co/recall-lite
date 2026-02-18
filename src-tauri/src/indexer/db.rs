use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use arrow_array::{
    Float32Array, FixedSizeListArray, Int64Array, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::connection::Connection;
use lancedb::index::Index;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::Table;

pub struct Record {
    pub path: String,
    pub content: String,
    pub vector: Vec<f32>,
    pub mtime: i64,
}

pub struct PendingChunk {
    pub path: String,
    pub content: String,
    pub mtime: i64,
}

pub async fn reset_index(db_path: &Path, table_name: &str) -> Result<()> {
    let db = lancedb::connect(&db_path.to_string_lossy())
        .execute()
        .await?;
    let _ = db.drop_table(table_name, &[]).await;
    Ok(())
}

pub async fn build_ann_index(table: &Table) -> Result<()> {
    table
        .create_index(&["vector"], Index::Auto)
        .execute()
        .await?;
    Ok(())
}

pub async fn build_fts_index(table: &Table) -> Result<()> {
    let _ = table
        .create_index(&["content"], Index::FTS(Default::default()))
        .execute()
        .await;
    Ok(())
}

pub async fn get_single_file_mtime(table: &Table, file_path: &str) -> Result<Option<i64>> {
    let safe_path = file_path.replace('\'', "''");
    let results = table
        .query()
        .only_if(format!("path = '{}'", safe_path))
        .select(lancedb::query::Select::Columns(vec!["mtime".to_string()]))
        .limit(1)
        .execute()
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    for batch in results {
        if let Some(mtime_array) = batch
            .column_by_name("mtime")
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>())
        {
            if batch.num_rows() > 0 {
                return Ok(Some(mtime_array.value(0)));
            }
        }
    }
    Ok(None)
}

pub async fn get_indexed_mtimes(table: &Table) -> Result<HashMap<String, i64>> {
    let mut mtimes = HashMap::new();

    let results = table
        .query()
        .select(lancedb::query::Select::Columns(vec![
            "path".to_string(),
            "mtime".to_string(),
        ]))
        .execute()
        .await?
        .try_collect::<Vec<_>>()
        .await?;

    for batch in results {
        let path_array = batch
            .column_by_name("path")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let mtime_array = batch
            .column_by_name("mtime")
            .and_then(|c| c.as_any().downcast_ref::<Int64Array>());

        if let (Some(paths), Some(mtimes_col)) = (path_array, mtime_array) {
            for i in 0..batch.num_rows() {
                mtimes.insert(paths.value(i).to_string(), mtimes_col.value(i));
            }
        }
    }

    Ok(mtimes)
}

pub async fn get_or_create_table(db: &Connection, table_name: &str, dim: usize) -> Result<Table> {
    match db.open_table(table_name).execute().await {
        Ok(table) => {
            let schema = table.schema().await?;
            let has_mtime = schema.field_with_name("mtime").is_ok();
            if let Ok(field) = schema.field_with_name("vector") {
                if let DataType::FixedSizeList(_, size) = field.data_type() {
                    if *size == dim as i32 && has_mtime {
                        return Ok(table);
                    }
                }
            }
            let _ = db.drop_table(table_name, &[]).await;
        }
        Err(_) => {}
    }

    let schema = Arc::new(make_schema(dim));

    let table = db
        .create_table(table_name, RecordBatchIterator::new(vec![], schema))
        .execute()
        .await?;

    Ok(table)
}

fn make_schema(dim: usize) -> Schema {
    Schema::new(vec![
        Field::new("path", DataType::Utf8, false),
        Field::new("content", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dim as i32,
            ),
            false,
        ),
        Field::new("mtime", DataType::Int64, false),
    ])
}

pub fn create_record_batch(records: Vec<Record>) -> Result<RecordBatch> {
    if records.is_empty() {
        return Err(anyhow!("No records to convert"));
    }

    let dim = records[0].vector.len();
    let schema = Arc::new(make_schema(dim));

    let paths: Vec<String> = records.iter().map(|r| r.path.clone()).collect();
    let contents: Vec<String> = records.iter().map(|r| r.content.clone()).collect();
    let mtimes: Vec<i64> = records.iter().map(|r| r.mtime).collect();

    let mut flat_vectors = Vec::with_capacity(records.len() * dim);
    for r in &records {
        flat_vectors.extend_from_slice(&r.vector);
    }

    let vector_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        dim as i32,
        Arc::new(Float32Array::from(flat_vectors)),
        None,
    )?;

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(StringArray::from(paths)),
            Arc::new(StringArray::from(contents)),
            Arc::new(vector_array),
            Arc::new(Int64Array::from(mtimes)),
        ],
    )
    .map_err(|e| anyhow!(e))
}
