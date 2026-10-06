//! 有界读页（§6.2）：读之前先按行数预留字节额度，额度不够就缩窗重读。
//!
//! 缩窗是安全的：keyset 分页的游标只在本批确认之后才前进，重读同一个游标位置
//! 既不重复写入也不跳过任何行。只有「1 行仍超限」才明确失败——缓冲不会为了塞下
//! 某一页而扩张（§6.2 禁止用扩张缓冲绕过上限）。

use datazen_driver_api::{ConnectionHandle, DatabaseDriver, TableSchema, Value};

use crate::error::TransferError;
use crate::execute::map_row_values;
use crate::model::ColumnMapping;
use crate::recordset::SourceScope;
use crate::resume::fingerprint::{build_page_for_context, validate_page};

use super::budget::PipelineBudget;
use super::{row_bytes, value_bytes, PIPELINE_INITIAL_BYTES};

/// 读一页所需的源端引用。
///
/// 显式列出而不是整个 `BoundedPipelineContext`：上下文里有 `&mut` checkpoint，
/// 借用它跨 `.await` 会让 future 失去 `Send`。
pub struct PageSource<'a> {
    pub driver: &'a dyn DatabaseDriver,
    pub handle: &'a ConnectionHandle,
    pub scope: &'a SourceScope,
    pub quote: char,
    pub schema: &'a TableSchema,
    pub columns: &'a [&'a ColumnMapping],
}

/// 一页读回并归一化后的行，以及它们在字节账里占的位置。
pub struct PreparedPage {
    /// 读回来的解码行：末行提供下一批游标，确认之前不得推进。
    pub rows: Vec<Vec<Option<Value>>>,
    /// 过完 IR 映射的行：送进 `bound_insert_batch`。
    pub projected: Vec<Vec<Option<Value>>>,
    pub page_bytes: usize,
    pub converted_bytes: usize,
}

impl PreparedPage {
    /// 确认批已提交后释放解码行占用的额度：此后只剩转换副本与待发送参数。
    pub fn release_source_rows(&mut self, budget: &mut PipelineBudget) {
        self.rows.clear();
        self.rows.shrink_to_fit();
        budget.release(self.page_bytes);
    }
}

/// 在字节额度内读一页并归一化；返回 `None` 表示游标已到末尾。
///
/// 任何失败都以 `Err` 交给调用方记 `terminal_error`：这样已确认的批边界仍然留在
/// 结果里（§7 要求保留已确认部分），而不是变成整表回滚。
pub async fn read_page_within_budget<'a>(
    source: PageSource<'a>,
    budget: &mut PipelineBudget,
    select_from: &str,
    projection: &[String],
    keys: &[String],
    cursor: Option<&[Value]>,
    limit: u32,
) -> Result<Option<PreparedPage>, TransferError> {
    let mut window = limit;
    loop {
        let rows_requested = budget.affordable_rows(window);
        let reserved = budget.reserve_page(rows_requested);
        let page = read_one_page(
            &source,
            select_from,
            projection,
            keys,
            cursor,
            rows_requested,
            reserved,
            budget,
        )
        .await?;
        let Some(page) = page else {
            return Ok(None);
        };
        let page_len = page.rows.len();
        let page_bytes: usize = page.rows.iter().map(|row| row_bytes(row)).sum();
        let projected = match page
            .rows
            .iter()
            .map(|row| map_row_values(&row[..source.columns.len()], source.schema, source.columns))
            .collect::<Result<Vec<_>, _>>()
        {
            Ok(rows) => rows,
            Err(error) => {
                budget.release(reserved);
                return Err(error);
            }
        };
        let converted_bytes: usize = projected.iter().map(|row| row_bytes(row)).sum();
        // 实测先收敛行估计，再按实际占用记账：此刻解码行与转换副本同时活着。
        budget.observe_page(page_bytes, page_len);
        budget.release(reserved);
        if budget.try_account(page_bytes) && budget.try_account(converted_bytes) {
            return Ok(Some(PreparedPage {
                rows: page.rows,
                projected,
                page_bytes,
                converted_bytes,
            }));
        }
        // 缩窗重读前把这页已记的两笔退干净：只退记账、不退额度，下一轮就会拿着
        // 上一轮的行去比剩余空间，把本来放得下的一行误判成超限。
        budget.release(page_bytes + converted_bytes);
        if rows_requested <= 1 {
            return Err(TransferError::validation(format!(
                "one source row needs {} bytes of pipeline buffer, over the {} byte bound ({}); migrate a narrower table or a smaller value",
                page_bytes + converted_bytes,
                PIPELINE_INITIAL_BYTES,
                budget.capacity()
            )));
        }
        window = (rows_requested / 2).max(1);
    }
}

/// 读一页原始解码行；空页与失败路径都退还预留额度。
async fn read_one_page(
    source: &PageSource<'_>,
    select_from: &str,
    projection: &[String],
    keys: &[String],
    cursor: Option<&[Value]>,
    rows_requested: u32,
    reserved: usize,
    budget: &mut PipelineBudget,
) -> Result<Option<datazen_driver_api::QueryResult>, TransferError> {
    let query = build_page_for_context(
        source.driver,
        select_from,
        source.scope,
        keys,
        cursor,
        rows_requested,
        source.quote,
        source.schema,
    )?;
    let page = match source
        .driver
        .query_with_params(source.handle, &query.0, &query.1)
        .await
    {
        Ok(page) => page,
        Err(error) => {
            budget.release(reserved);
            return Err(TransferError::unsupported(format!(
                "bounded source page read failed: {error}"
            )));
        }
    };
    // 单值超限明确失败、不扩张缓冲绕过限制。
    if page.rows.iter().any(|row| {
        row.iter()
            .any(|value| value_bytes(value.as_ref()) > PIPELINE_INITIAL_BYTES)
    }) {
        budget.release(reserved);
        return Err(TransferError::validation(
            "source row contains a single value exceeding the 8 MiB pipeline buffer bound",
        ));
    }
    if let Err(error) = validate_page(&page, projection, rows_requested as usize) {
        budget.release(reserved);
        return Err(error);
    }
    if page.rows.is_empty() {
        budget.release(reserved);
        return Ok(None);
    }
    budget.release(reserved);
    Ok(Some(page))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::Value;

    #[test]
    fn prepared_page_release_drops_the_source_rows_only() {
        let mut budget = PipelineBudget::new();
        assert!(budget.try_account(4096));
        let mut page = PreparedPage {
            rows: vec![vec![Some(Value::Null)]],
            projected: vec![vec![Some(Value::Null)]],
            page_bytes: 4096,
            converted_bytes: 0,
        };
        page.release_source_rows(&mut budget);
        assert_eq!(budget.used(), 0);
        assert!(page.projected.len() == 1, "converted rows stay in flight");
    }
}
