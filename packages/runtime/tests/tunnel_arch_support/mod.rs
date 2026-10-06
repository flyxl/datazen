//! `ResourceManager` ⇄ `TunnelLedger` 接线契约的**共用替身**。
//!
//! 两个测试二进制共用本文件，但各自只用其中一部分，所以整文件豁免 dead_code
//! （与 `tests/support/mod.rs` 同一处理）。
//!
//! 三条纪律：
//!
//! 1. **没有真实等待**：时间走 [`TestClock`]，整条链是同步的，没有偶发失败。
//! 2. **替身不带计数**：[`RecordingTunnelPort`] 只记物理开合，引用计数的唯一权威是
//!    `TunnelLedger`；这里若偷偷加一个 `Mutex<usize>` 就等于伪造了第二个计数器。
//! 3. **手写替身**：`datazen_runtime::testing` 下的 fixture 由 `cfg(test)` 门控，而
//!    `tests/` 编译 lib 时并不带 `cfg(test)`（同 `resource_return_to_pool.rs` 的说明）。
#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use datazen_platform_api::id::{NetworkRouteRef, NetworkRouteRevision};
use datazen_platform_api::ports::network::TunnelSpec;
use datazen_runtime::connection::port::CancelDisposition;
use datazen_runtime::connection::types::ClientInstanceId;
use datazen_runtime::connection::{
    ConfigRevision, ConnectionId, ExecutionId, NamespaceTarget, OwnerRef, PoolKeyInputs, ResourceId,
};
use datazen_runtime::resource::{
    LeasePurpose, LeaseRequest, MonotonicSource, PhysicalTransport, ResourceError, ResourceManager,
};
use datazen_runtime::tunnel::{TunnelError, TunnelFault, TunnelHandle, TunnelTransport};

// ------------------------------------------------------------ 时间

/// 手动推进的单调时钟：不出现 `sleep`，每一步断言都确定。
pub struct TestClock {
    nanos: AtomicU64,
}

impl TestClock {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            nanos: AtomicU64::new(0),
        })
    }
}

impl MonotonicSource for TestClock {
    fn now_nanos(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst)
    }
}

// ------------------------------------------------------------ 物理端口替身

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysicalEvent {
    Open(ResourceId),
    Close(ResourceId),
}

#[derive(Debug, Default)]
struct PhysicalJournal {
    events: Vec<PhysicalEvent>,
    issued: u64,
    open_fails: bool,
    close_fails: bool,
    reset_fails: bool,
}

/// 可脚本化的物理端口：开 / 关 / 复位三个阶段各自可注入失败。
pub struct ScriptedTransport {
    journal: Mutex<PhysicalJournal>,
}

impl ScriptedTransport {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            journal: Mutex::new(PhysicalJournal::default()),
        })
    }

    fn lock(&self) -> MutexGuard<'_, PhysicalJournal> {
        // 断言失败一次不该把后续断言全变成 panic 噪声，所以不传播 poison。
        self.journal
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub fn fail_open(&self) {
        self.lock().open_fails = true;
    }

    pub fn fail_close(&self) {
        self.lock().close_fails = true;
    }

    /// 端口恢复：排障之后关闭重新能被确认，用来验证「重试一次就成对释放」。
    pub fn succeed_close(&self) {
        self.lock().close_fails = false;
    }

    /// 复位结果不明（CM-73）：处置必须从 `Closed` 升级成 `Quarantined`。
    pub fn fail_reset(&self) {
        self.lock().reset_fails = true;
    }

    pub fn events(&self) -> Vec<PhysicalEvent> {
        self.lock().events.clone()
    }

    pub fn opened(&self) -> usize {
        self.lock()
            .events
            .iter()
            .filter(|event| matches!(event, PhysicalEvent::Open(_)))
            .count()
    }

    pub fn closed(&self) -> usize {
        self.lock()
            .events
            .iter()
            .filter(|event| matches!(event, PhysicalEvent::Close(_)))
            .count()
    }
}

impl PhysicalTransport for ScriptedTransport {
    fn open(&self, _request: &LeaseRequest) -> Result<ResourceId, ResourceError> {
        let mut journal = self.lock();
        if journal.open_fails {
            return Err(ResourceError::TransportRefused("socket open refused"));
        }
        journal.issued += 1;
        let resource_id = ResourceId::new(format!("res-{}", journal.issued));
        journal
            .events
            .push(PhysicalEvent::Open(resource_id.clone()));
        Ok(resource_id)
    }

    fn close(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        let mut journal = self.lock();
        journal
            .events
            .push(PhysicalEvent::Close(resource_id.clone()));
        if journal.close_fails {
            return Err(ResourceError::TransportRefused("socket close unconfirmed"));
        }
        Ok(())
    }

    fn reset(&self, _resource_id: &ResourceId) -> Result<(), ResourceError> {
        if self.lock().reset_fails {
            return Err(ResourceError::TransportRefused("session reset unconfirmed"));
        }
        Ok(())
    }

    fn cancel(
        &self,
        _resource_id: &ResourceId,
        _execution_id: &ExecutionId,
    ) -> Result<CancelDisposition, ResourceError> {
        Ok(CancelDisposition::Requested)
    }
}

// ------------------------------------------------------------ 隧道端口替身

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelEvent {
    Open,
    Close,
}

/// 隧道端口替身：**不带任何引用计数**。唯一计数在台账里，端口只记物理开合。
pub struct RecordingTunnelPort {
    events: Mutex<Vec<TunnelEvent>>,
    open_fails: Mutex<bool>,
}

impl RecordingTunnelPort {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            events: Mutex::new(Vec::new()),
            open_fails: Mutex::new(false),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Vec<TunnelEvent>> {
        self.events
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }

    pub fn fail_open(&self) {
        *self.open_fails.lock().unwrap_or_else(|e| e.into_inner()) = true;
    }

    pub fn opened(&self) -> usize {
        self.lock()
            .iter()
            .filter(|e| **e == TunnelEvent::Open)
            .count()
    }

    pub fn closed(&self) -> usize {
        self.lock()
            .iter()
            .filter(|e| **e == TunnelEvent::Close)
            .count()
    }
}

impl TunnelTransport for RecordingTunnelPort {
    fn open(&self, spec: &TunnelSpec) -> Result<Option<TunnelHandle>, TunnelError> {
        if *self.open_fails.lock().unwrap_or_else(|e| e.into_inner()) {
            return Err(TunnelError::Transport {
                spec: spec.clone(),
                fault: TunnelFault::Open,
            });
        }
        self.lock().push(TunnelEvent::Open);
        Ok(Some(TunnelHandle::new()))
    }

    fn close(&self, _spec: &TunnelSpec) -> Result<(), TunnelError> {
        self.lock().push(TunnelEvent::Close);
        Ok(())
    }

    fn revision(&self, _route_ref: &NetworkRouteRef) -> Result<NetworkRouteRevision, TunnelError> {
        Ok(NetworkRouteRevision::new(1))
    }
}

// ------------------------------------------------------------ 申请构造

pub fn connection(id: &str) -> ConnectionId {
    ConnectionId::new(id.to_owned())
}

fn owner() -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-arch-1"),
    }
}

pub fn request(connection_id: &ConnectionId, database: &str) -> LeaseRequest {
    LeaseRequest::new(
        PoolKeyInputs {
            connection_id: connection_id.clone(),
            config_revision: ConfigRevision::new(1),
            driver_id: "postgres".to_owned(),
            namespace: NamespaceTarget {
                database: database.to_owned(),
                catalog: "appdb".to_owned(),
                schema: "public".to_owned(),
                path: "appdb.public".to_owned(),
            },
            execution_identity_key: "owner-hash-1".to_owned(),
            policy_isolation_key: "policy-a".to_owned(),
        },
        owner(),
        LeasePurpose::ShortOperation,
    )
}

pub fn tunnel_spec() -> TunnelSpec {
    TunnelSpec::new(
        NetworkRouteRef::new("route://bastion/prod"),
        "jump.internal",
        22,
    )
}

/// 装好隧道端口的管理器：三件替身都交出去，测试自己要用。
pub fn wired() -> (
    ResourceManager,
    Arc<ScriptedTransport>,
    Arc<RecordingTunnelPort>,
    ConnectionId,
) {
    let transport = ScriptedTransport::new();
    let clock = TestClock::new();
    let tunnel = RecordingTunnelPort::new();
    let connection_id = connection("conn-arch");
    let manager =
        ResourceManager::new(transport.clone(), clock).with_tunnel_transport(tunnel.clone());
    (manager, transport, tunnel, connection_id)
}

/// 未装隧道端口的管理器（直连世界的基线）。
pub fn unwired() -> (ResourceManager, Arc<ScriptedTransport>, ConnectionId) {
    let transport = ScriptedTransport::new();
    let clock = TestClock::new();
    let connection_id = connection("conn-arch");
    (
        ResourceManager::new(transport.clone(), clock),
        transport,
        connection_id,
    )
}
