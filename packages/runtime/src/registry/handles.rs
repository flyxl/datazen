//! §6.5 会话级资源句柄登记 + 取消绑定。
//!
//! ## 两条独立的表
//!
//! | 表 | 装什么 | 生命周期 |
//! | --- | --- | --- |
//! | `handles` | §6.5 的 `SessionHandleRef`（事务/游标/服务端预编译） | 直到在**原资源**上被终结 |
//! | `bindings` | 每次执行 → 精确 `cancelHandle` 的绑定 | 直到该执行离开 in-flight |
//!
//! 合成一张表是错的：句柄要活到资源关闭，取消绑定只在执行 in-flight 期间有意义。
//! 一旦合成，取消一个已完成的执行就会把仍然有效的句柄一起清掉，
//! 宿主账上的「活句柄数」随即失真，§9.4 的归池前检查随之失效。
//!
//! ## 登记是内存态，且随 actor 一起死
//!
//! §6.5 / §4.5：登记**不落盘**，actor 终止后**不得重建**句柄，
//! 恢复只能靠显式新建一个 `dbSessionId`。因此本结构体没有任何持久化接口。
//!
//! ## 伪造绑定的处理
//!
//! 一个过期的 `cancelHandle` 打到**新**的执行上，是 §3.2 L143 明令拒绝的。
//! 正确处置是拒绝并**保持绑定表不变**——如果顺手把新执行的绑定替换成旧 handle，
//! 这次取消就会真的打中新执行，而调用方以为自己在取消旧的那次。绑定校验因此是
//! 纯查表（[`HandleRegistry::check_binding`]），它在**任何后端调用之前**执行。

use crate::connection::{ExecutionId, HandleId, SessionHandleRef};

use crate::connection::RuntimeError;

/// 一次执行与它的精确 cancelHandle 绑定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionBinding {
    pub execution_id: ExecutionId,
    /// §3.2 L143 的精确 handle。**不进审计**（它是驱动侧的控制凭据）。
    pub cancel_handle: String,
    /// 绑定建立时的资源标识。资源被替换后旧绑定自动失效。
    pub resource_id: String,
    pub context_revision: u64,
}

/// §6.5 句柄登记 + 取消绑定。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HandleRegistry {
    handles: Vec<SessionHandleRef>,
    bindings: Vec<ExecutionBinding>,
}

impl HandleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// §6.5：登记一批句柄。**必须**在把执行终态返回给调用方之前调用。
    ///
    /// 返回登记后的宿主账总数。重复的 `handle_id` 整批拒绝——
    /// 「一个 id 只能登记一次」是去重的前提；让重复项混进来，
    /// 归池前检查就会数出比实际多出来的活句柄，于是**永远不放行**，
    /// 或者更糟：放行了却留下一条没人负责的句柄。
    pub fn register(&mut self, handles: Vec<SessionHandleRef>) -> Result<usize, RuntimeError> {
        let mut seen: Vec<&HandleId> = self.handles.iter().map(|held| &held.handle_id).collect();
        for candidate in &handles {
            if seen.contains(&&candidate.handle_id) {
                return Err(RuntimeError::InvariantBroken("duplicateHandleId"));
            }
            seen.push(&candidate.handle_id);
        }
        self.handles.extend(handles);
        Ok(self.handles.len())
    }

    /// 取出并清空全部登记句柄，供在**原资源**上终结（§9.4 释放顺序的第一步）。
    ///
    /// ⚠ **不要用这条去做归池前检查。** 它把「取走」和「注销」合成一个动作：账
    /// 立刻空了，`handle_count()` 之后恒为 0，于是 §9.4「宿主账上还有已登记句柄 ⇒
    /// 检查必须失败」在这条路径上**永远不可能**被触发——哪怕一个句柄都没被真正终结。
    /// 唯一的释放例程因此改走 [`HandleRegistry::retire`]：只注销**确认终结**的那批。
    /// 这里保留只读的 [`HandleRegistry::handles`] 供终结批次取样。
    pub fn drain_handles(&mut self) -> Vec<SessionHandleRef> {
        std::mem::take(&mut self.handles)
    }

    /// 把**已在原资源上确认终结**的那批句柄从宿主账上注销，其余留在账上。
    ///
    /// 为什么必须「确认了才注销」而不是「取走就算注销」：§9.4 的归池条件是
    /// **宿主自己**判定已登记句柄为空（driver 对已交出的句柄没有可见性，它的
    /// `Clean` 不构成事务终结的证据）。如果释放例程在发出终结请求之前就把账清空，
    /// 这个判定就被取样动作本身满足了——一个恒真的检查不是检查。留下未确认的句柄，
    /// [`HandleRegistry::handle_count`] 才会非零，`CloseResource.registered_handles`
    /// 才会带着真实数字出去，墓碑与审计也才会落到 `Lost` / `Undecided`。
    ///
    /// 注销按 `handle_id` 精确匹配；不在账上的 id 直接忽略（重复注销不得二次生效）。
    /// 返回注销之后账上剩余的句柄数。
    pub fn retire(&mut self, finalized: &[SessionHandleRef]) -> usize {
        let retired: Vec<&HandleId> = finalized.iter().map(|handle| &handle.handle_id).collect();
        self.handles
            .retain(|held| !retired.contains(&&held.handle_id));
        self.handles.len()
    }

    /// 当前登记的句柄（只读视图；不给 `&mut`，防止调用方绕开注销顺序）。
    pub fn handles(&self) -> &[SessionHandleRef] {
        &self.handles
    }

    pub fn handle_count(&self) -> usize {
        self.handles.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handles.is_empty()
    }

    /// 为一次刚完成的执行建立取消绑定。
    pub fn bind_execution(&mut self, binding: ExecutionBinding) {
        self.bindings.push(binding);
    }

    /// 该执行是否仍在 in-flight 且绑定可用。
    pub fn is_bound(&self, execution_id: &ExecutionId) -> bool {
        self.bindings
            .iter()
            .any(|binding| &binding.execution_id == execution_id)
    }

    /// 取消前的绑定校验（§3.2 L143）。
    ///
    /// 三种拒绝，各自的原因码不同：
    ///
    /// | 情形 | 原因码 | 为什么必须分开 |
    /// | --- | --- | --- |
    /// | 执行没有绑定 | `unboundExecution` | 调用方发的是不存在的执行 id |
    /// | 绑定在、handle 对不上 | `cancelBindingMismatch` | **过期的 handle 打到了新执行上** |
    /// | 绑定的资源已经不是当前资源 | `cancelBindingMismatch` | 资源已替换，旧绑定失效 |
    ///
    /// 校验是纯查表：它**不修改**任何状态。伪造的取消请求打过来之后，
    /// 绑定表必须与请求前逐字相同——否则一次失败的取消就会污染后续的取消。
    pub fn check_binding(
        &self,
        execution_id: &ExecutionId,
        cancel_handle: &str,
        current_resource_id: &str,
    ) -> Result<(), RuntimeError> {
        let binding = self
            .bindings
            .iter()
            .find(|binding| &binding.execution_id == execution_id)
            .ok_or(RuntimeError::CancelFailed("unboundExecution"))?;
        if binding.cancel_handle != cancel_handle {
            return Err(RuntimeError::CancelFailed("cancelBindingMismatch"));
        }
        if binding.resource_id != current_resource_id {
            return Err(RuntimeError::CancelFailed("cancelBindingMismatch"));
        }
        Ok(())
    }

    /// 执行离开 in-flight（拿到终态）后清掉它的绑定。
    ///
    /// 注意：这里**只清绑定，不清句柄**——§6.5 明确执行终态不使句柄失效。
    pub fn release_execution(&mut self, execution_id: &ExecutionId) {
        self.bindings
            .retain(|binding| &binding.execution_id != execution_id);
    }

    /// 资源被替换：清掉全部绑定（旧资源的绑定对新资源无意义）。
    ///
    /// 句柄**不清**——它们必须在**原资源**上先被终结，清在这里等于跳过回滚。
    pub fn invalidate_bindings(&mut self) {
        self.bindings.clear();
    }

    pub fn binding_count(&self) -> usize {
        self.bindings.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{Counter, HandleId, HandleKind, ResourceId};

    /// `unwrap` 的具名替身：失败时把上下文一起打出来，而不是裸 panic。
    fn ok<T, E: std::fmt::Debug>(result: Result<T, E>, context: &str) -> T {
        result.unwrap_or_else(|error| panic!("{context}: {error:?}"))
    }

    fn handle(name: &str) -> SessionHandleRef {
        SessionHandleRef::new(
            HandleId::new(name),
            HandleKind::Transaction,
            ResourceId::new("res_1"),
            Counter::new(3),
        )
    }

    fn binding(execution_id: &str, cancel_handle: &str) -> ExecutionBinding {
        ExecutionBinding {
            execution_id: ExecutionId::new(execution_id),
            cancel_handle: cancel_handle.to_owned(),
            resource_id: "res_1".to_owned(),
            context_revision: 0,
        }
    }

    /// §6.5：执行终态**不使**句柄失效，只有在原资源上终结才清账。
    #[test]
    fn terminal_execution_does_not_invalidate_registered_handles() {
        let mut registry = HandleRegistry::new();
        ok(registry.register(vec![handle("h_1")]), "登记 h_1");
        registry.bind_execution(binding("exec_1", "ch_1"));
        registry.release_execution(&ExecutionId::new("exec_1"));
        assert_eq!(registry.handle_count(), 1, "终态不得让句柄失效");
        assert_eq!(registry.binding_count(), 0, "绑定才随执行结束而清");
        assert!(!registry.is_empty());
    }

    /// CM-24：过期的 cancelHandle 打到新执行上必须被拒，且**不产生任何后端调用**。
    ///
    /// 关键在后半句：绑定表必须与请求前逐字相同。
    /// 如果实现顺手把新绑定的 handle 改成旧值，这次取消就会真的打中新执行，
    /// 而调用方以为自己在取消旧的那次执行——这是最隐蔽的一种串扰。
    #[test]
    fn forged_binding_is_rejected_and_leaves_no_dirty_mapping() {
        let mut registry = HandleRegistry::new();
        registry.bind_execution(binding("exec_old", "ch_old"));
        registry.release_execution(&ExecutionId::new("exec_old"));
        registry.bind_execution(binding("exec_new", "ch_new"));

        let before = registry.clone();
        assert_eq!(
            registry
                .check_binding(&ExecutionId::new("exec_new"), "ch_old", "res_1")
                .unwrap_err(),
            RuntimeError::CancelFailed("cancelBindingMismatch"),
            "旧 handle 打新执行必须被拒"
        );
        assert_eq!(registry, before, "被拒的取消不得在绑定表里留下任何痕迹");
        // 旧执行仍然查得到「未绑定」而不是被复活。
        assert_eq!(
            registry
                .check_binding(&ExecutionId::new("exec_old"), "ch_old", "res_1")
                .unwrap_err(),
            RuntimeError::CancelFailed("unboundExecution"),
            "旧执行已被释放，不得因为一次被拒的取消而复活"
        );
    }

    /// 资源被替换后旧绑定失效，且句柄**不被清空**（必须先在原资源上终结）。
    #[test]
    fn resource_replacement_invalidates_bindings_but_keeps_handles() {
        let mut registry = HandleRegistry::new();
        ok(
            registry.register(vec![handle("h_1"), handle("h_2")]),
            "登记 h_1+h_2",
        );
        registry.bind_execution(binding("exec_1", "ch_1"));
        registry.invalidate_bindings();
        assert_eq!(registry.binding_count(), 0);
        assert_eq!(
            registry.handle_count(),
            2,
            "替换不得就地清句柄——跳过回滚等于把活事务留在旧资源上"
        );
        assert_eq!(
            registry
                .check_binding(&ExecutionId::new("exec_1"), "ch_1", "res_2")
                .unwrap_err(),
            RuntimeError::CancelFailed("unboundExecution")
        );
    }

    /// 句柄取走即清账：`drain_handles` 之后宿主账必为 0（§9.4 放行条件）。
    #[test]
    fn draining_handles_clears_the_host_ledger() {
        let mut registry = HandleRegistry::new();
        ok(registry.register(vec![handle("h_1")]), "登记 h_1");
        let drained = registry.drain_handles();
        assert_eq!(drained.len(), 1);
        assert!(registry.is_empty());
        assert!(crate::registry::backend::ready_to_return_to_pool(
            registry.handle_count()
        ));
    }
}
