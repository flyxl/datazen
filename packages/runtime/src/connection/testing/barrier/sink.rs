//! 供 provider / 测试使用的 `ResultSink` 载体。

use crate::connection::execution::{ExecutionErrorCode, ResultSink, SinkWrite, TruncationReason};
use crate::connection::types::ExecutionId;

// ---------------------------------------------------------------------------
// 供 provider / 测试使用的 sink
// ---------------------------------------------------------------------------

/// 收集型 sink：把写入落到内存，供断言「已产出字节」。
#[derive(Debug, Default)]
pub struct CollectingSink {
    produced_bytes: u64,
    events: u64,
    completed: Option<u64>,
    failure: Option<ExecutionErrorCode>,
}

impl CollectingSink {
    pub fn produced_bytes(&self) -> u64 {
        self.produced_bytes
    }

    pub fn events(&self) -> u64 {
        self.events
    }

    pub fn completed_at(&self) -> Option<u64> {
        self.completed
    }

    pub fn failure(&self) -> Option<ExecutionErrorCode> {
        self.failure
    }
}

impl ResultSink for CollectingSink {
    fn write(
        &mut self,
        _execution_id: &ExecutionId,
        _chunk_index: crate::connection::types::Counter,
        bytes: usize,
    ) -> SinkWrite {
        self.produced_bytes += bytes as u64;
        self.events += 1;
        SinkWrite::Accepted
    }

    fn complete(&mut self, _execution_id: &ExecutionId, produced_bytes: u64) {
        self.completed = Some(produced_bytes);
    }

    fn fail(&mut self, _execution_id: &ExecutionId, code: ExecutionErrorCode, produced_bytes: u64) {
        self.failure = Some(code);
        self.completed = Some(produced_bytes);
    }
}

/// 写失败型 sink：「`ResultSink` 写入失败」的载体。
#[derive(Debug, Default)]
pub struct FailingSink {
    pub code: Option<ExecutionErrorCode>,
}

impl ResultSink for FailingSink {
    fn write(
        &mut self,
        _execution_id: &ExecutionId,
        _chunk_index: crate::connection::types::Counter,
        _bytes: usize,
    ) -> SinkWrite {
        SinkWrite::Truncated(TruncationReason::ProducerWriteFailed)
    }

    fn complete(&mut self, _execution_id: &ExecutionId, produced_bytes: u64) {
        let _ = produced_bytes;
    }

    fn fail(&mut self, _execution_id: &ExecutionId, code: ExecutionErrorCode, produced_bytes: u64) {
        self.code = Some(code);
        let _ = produced_bytes;
    }
}
