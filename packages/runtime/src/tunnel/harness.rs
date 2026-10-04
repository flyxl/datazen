//! 隧道旅程的共享替身 —— 与 `resource::harness` 同一形态：
//! 记录式物理端口 + 可注入故障，**不用 sleep、不连真网络**。

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use datazen_platform_api::id::{NetworkRouteRef, NetworkRouteRevision};
use datazen_platform_api::ports::network::TunnelSpec;

use crate::connection::types::LeaseId;
use crate::tunnel::{TunnelError, TunnelFault, TunnelHandle, TunnelLedger, TunnelTransport};

/// 物理端口做过的每一件事。CM-32 的「不关闭 / 最后一个引用才关闭」断言
/// **全靠读这个 journal 的计数**，而不是只看返回值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TunnelEvent {
    Open(TunnelSpec),
    Close(TunnelSpec),
    /// transport 判定这条 `spec` 不需要隧道（等价宿主 `TunnelKind::None` 直连）。
    OpenWithoutTunnel(TunnelSpec),
}

impl TunnelEvent {
    pub fn spec(&self) -> &TunnelSpec {
        match self {
            Self::Open(spec) | Self::Close(spec) | Self::OpenWithoutTunnel(spec) => spec,
        }
    }
}

/// 记录式隧道物理端口。
///
/// **它不持有任何引用计数** —— 见 `tunnel::transport` 模块头。
/// 这是刻意的：`single_counter_algebra_holds` 要证明的正是「只有一份账」，
/// 替身自己一旦也数一遍，这条不变量就永远证明不了。
pub struct RecordingTunnelTransport {
    journal: Mutex<Vec<TunnelEvent>>,
    /// 故障注入用 `Vec` 而非 map：冻结的 `TunnelSpec` 只派生
    /// `PartialEq`/`Eq`，没有 `Ord`/`Hash`，另造键类型等于另造一份共享定义。
    open_faults: Mutex<Vec<TunnelSpec>>,
    close_faults: Mutex<Vec<TunnelSpec>>,
    untunneled: Mutex<Vec<TunnelSpec>>,
    revisions: Mutex<BTreeMap<String, u64>>,
}

impl RecordingTunnelTransport {
    pub fn new() -> Self {
        Self {
            journal: Mutex::new(Vec::new()),
            open_faults: Mutex::new(Vec::new()),
            close_faults: Mutex::new(Vec::new()),
            untunneled: Mutex::new(Vec::new()),
            revisions: Mutex::new(BTreeMap::new()),
        }
    }

    /// 让此后所有次 `open(spec)` 失败。
    pub fn fail_open(&self, spec: &TunnelSpec) -> &Self {
        self.open_faults
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(spec.clone());
        self
    }

    /// 让此后所有次 `close(spec)` 失败 —— 拆除结果不明。
    pub fn fail_close(&self, spec: &TunnelSpec) -> &Self {
        self.close_faults
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(spec.clone());
        self
    }

    /// 声明这条 `spec` **不经隧道**（等价宿主 `TunnelKind::None` 直连）。
    pub fn without_tunnel(&self, spec: &TunnelSpec) -> &Self {
        self.untunneled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(spec.clone());
        self
    }

    /// 设定路由版本。`bump` 一调即模拟路由轮换。
    pub fn set_revision(&self, route_ref: &NetworkRouteRef, value: u64) -> &Self {
        self.revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(route_ref.as_str().to_string(), value);
        self
    }

    pub fn journal(&self) -> Vec<TunnelEvent> {
        self.journal
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// `close` 被调用的**次数** —— CM-32 第二条断言的主观测点。
    pub fn close_calls(&self) -> usize {
        self.journal()
            .iter()
            .filter(|event| matches!(event, TunnelEvent::Close(_)))
            .count()
    }

    /// `open` 被调用的**次数**（只数真正开出隧道的那些调用）。
    pub fn open_calls(&self) -> usize {
        self.journal()
            .iter()
            .filter(|event| matches!(event, TunnelEvent::Open(_)))
            .count()
    }

    /// 物理接缝被**触碰**的总次数：`open` / `close` / 「这条 spec 不需要隧道」。
    ///
    /// 与 [`Self::open_calls`] 的差别正是「台账问了但没建隧道」这一种情形，
    /// 计数断言必须能把它单独看见。
    pub fn consulted(&self) -> usize {
        self.journal().len()
    }

    pub fn is_open(&self, spec: &TunnelSpec) -> bool {
        let journal = self.journal();
        let opened = journal
            .iter()
            .filter(|event| matches!(event, TunnelEvent::Open(s) if s == spec))
            .count();
        let closed = journal
            .iter()
            .filter(|event| matches!(event, TunnelEvent::Close(s) if s == spec))
            .count();
        opened > closed
    }
}

impl TunnelTransport for RecordingTunnelTransport {
    fn open(&self, spec: &TunnelSpec) -> Result<Option<TunnelHandle>, TunnelError> {
        if self
            .untunneled
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(spec)
        {
            self.journal
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(TunnelEvent::OpenWithoutTunnel(spec.clone()));
            return Ok(None);
        }
        if self
            .open_faults
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .any(|faulted| faulted == spec)
        {
            return Err(TunnelError::Transport {
                spec: spec.clone(),
                fault: TunnelFault::Open,
            });
        }
        self.journal
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(TunnelEvent::Open(spec.clone()));
        Ok(Some(TunnelHandle::new()))
    }

    fn close(&self, spec: &TunnelSpec) -> Result<(), TunnelError> {
        if self
            .close_faults
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .any(|faulted| faulted == spec)
        {
            return Err(TunnelError::Transport {
                spec: spec.clone(),
                fault: TunnelFault::Close,
            });
        }
        self.journal
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(TunnelEvent::Close(spec.clone()));
        Ok(())
    }

    fn revision(&self, route_ref: &NetworkRouteRef) -> Result<NetworkRouteRevision, TunnelError> {
        self.revisions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(route_ref.as_str())
            .copied()
            .map(NetworkRouteRevision::new)
            .ok_or(TunnelError::RevisionUnavailable)
    }
}

/// 隧道旅程夹具。
pub struct TunnelHarness {
    pub transport: Arc<RecordingTunnelTransport>,
    pub ledger: TunnelLedger,
    pub route_ref: NetworkRouteRef,
}

impl TunnelHarness {
    pub fn new() -> Self {
        let transport = Arc::new(RecordingTunnelTransport::new());
        Self {
            ledger: TunnelLedger::new(transport.clone()),
            transport,
            route_ref: NetworkRouteRef::new("route-alpha"),
        }
    }

    /// 一条 `TunnelSpec`：三个分量**全部**参与共享判定。
    pub fn spec(&self, via_host: &str, via_port: u16) -> TunnelSpec {
        TunnelSpec::new(self.route_ref.clone(), via_host.to_string(), via_port)
    }

    /// 同一条 spec 的便捷构造（同一跳板）。
    pub fn same_spec(&self) -> TunnelSpec {
        self.spec("jump.internal", 2222)
    }

    pub fn lease_id(&self, tag: &str) -> LeaseId {
        LeaseId::new(tag)
    }
}

impl Default for TunnelHarness {
    fn default() -> Self {
        Self::new()
    }
}
