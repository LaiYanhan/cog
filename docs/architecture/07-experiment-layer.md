# 第 6 层：实验层 — `src/experiment/` + `src/command/experiment_cmd.rs`

> 在潜空间中推演，而不是在代码空间中试错。Experiment 是**单根假设推理工具**——围绕一个不确定的变更点，模拟它的传播后果，完全不触碰真实模型，直到显式 commit。它是 [下降协议](../concepts/02-descent-protocol.md) Phase 2 的实现。

## 结构

```
src/experiment/
├── session.rs       # Experiment 主类型 + 生命周期方法
├── ops.rs           # ExperimentOp enum
├── report.rs        # ExperimentReport, Contradiction, CommitReport
└── persistence.rs   # .cog/experiments/<id>.json 序列化/恢复
```

`src/command/experiment_cmd.rs` 是 CLI 入口，编排 session 方法 + 持久化。

## Experiment 类型

```rust
pub struct Experiment {
    pub id: String,                      // UUID
    pub description: String,
    pub entity_focus: String,            // 推演焦点的 qualified name
    pub entity_focus_id: String,
    pub created_at: DateTime<Utc>,
    pub status: ExperimentStatus,
    pub ops: Vec<ExperimentOp>,          // 暂存的假设操作
    pub structure: StructureSpace,       // 结构子空间快照
    pub semantic: SemanticSpace,         // 语义子空间快照
    pub boundary_count: usize,           // 子图边界未加载数
    pub risk_score: Option<f64>,         // evaluate 后填充
    pub affected: Vec<AffectedAssertion>,
    pub contradictions: Vec<Contradiction>,
    pub saved: bool,                     // draft(false) vs checkpoint(true)
}
```

## ExperimentStatus

```rust
pub enum ExperimentStatus { Open, Evaluated, Committed, Discarded }
```

生命周期：`Open` →（`evaluate`）→ `Evaluated` →（`commit`）→ `Committed` 或（`discard`）→ `Discarded`。

**关键性质**：

- `hypothesize` 在 `Open` 和 `Evaluated` 状态下均可调用——agent 可在 SCOUT 发现新信息后追加假设重新推演。
- `mark_evaluated` 是幂等的——`Evaluated` → `Evaluated` 是 no-op。
- `evaluate` 要求状态为 `Open` 或 `Evaluated`。

## ExperimentOp（4 种）

```rust
pub enum ExperimentOp {
    Assertion { entity_name, kind, claim, grounds, depends_on: Option },
    Retraction { assertion_id, reason },
    Relation { from_entity, to_entity, kind },
    Delete { entity_name },
}
```

`assert --depends-on` 支持：TMS cascade 检测依赖 `depends_on` 链，不传则 evaluate 的 cascade_count 始终为 0。

## 生命周期方法

### `start(repo, entity_name, description, max_nodes)`

1. `resolve_entity` 定位焦点——精确或模糊后缀匹配。
2. **若实体不存在**：创建 provisional `Experiment` origin 实体作为焦点（空子图）。这允许 agent 对"即将创建的函数"做推演。
3. 若存在：`StructureSpace::load(repo, focus, 0, max_nodes)` 加载依赖子图（默认 cap 500）。
4. `SemanticSpace::load` 加载语义子空间。
5. 返回 `Open` 状态的 experiment（`saved: false`，即 draft）。

### `hypothesize(op)`

纯追加——把 `ExperimentOp` 压入 `ops`，**不触碰真实 repository**。

### `evaluate() -> ExperimentReport`

**纯计算**，不 mutate experiment。对每个 op 模拟效果：

- `Retraction` → `semantic.simulate_retract(assertion_id)` 得级联影响。
- `Assertion` → 检测矛盾：同 entity + 同 kind + 不同 claim 的 active assertion。
- `Delete` → 检测"删除会孤儿化哪些 assertion"。
- `Relation` → 无模拟效果。

并计算：`blind_entities`（子图中无 active assertion 的实体）、`risk_score`（`semantic.assess_risk`）。返回 `ExperimentReport { cascade_count, contradictions, blind_entities, boundary_count, ... }`。

### `commit(self, repo) -> CommitReport`

**确定性 replay**——在真实 repository 上按序回放 `ops`：

- `Assertion`：`resolve_entity`（后缀匹配）；找不到则 `upsert_entity(.., Experiment)` 物化 provisional 实体。再 `create_assertion`。
- `Retraction`：`resolve_assertion_id` + `CascadeEngine::retract`（真实级联）。找不到则 skip。
- `Delete`：`delete_entity`，找不到则 skip。
- `Relation`：两端实体须都存在，否则 skip。

返回 `CommitReport { ops_applied, ops_skipped, details }`。状态置 `Committed`。

> **为何 replay 而非 diff-merge**：确定性操作日志回放，不需要 UUID 冲突解决，比 diff-then-merge 更简单可靠。

### `discard(self)`

消费 self，状态置 `Discarded`，无副作用。**但实施过程中 assert 到真实模型的 fragility/correction 不会撤销**——失败回流机制（见下降协议）。

## 持久化

`persistence.rs` 序列化到 `.cog/experiments/<id>.json`，跨 session 恢复。draft（`saved: false`）自动持久化，避免丢失未保存工作；`save` 命令标记为 checkpoint（`saved: true`）。`list` 区分 draft/saved。

`ActiveExperiment`（在 `domain/report.rs`）是 `cog next` 检测活跃实验的轻量结构：`short_id`、`description`、`status`（"draft"/"evaluated"）、`mtime`。`next_cmd::detect_active_experiments` 扫描目录、按 mtime 排序。

## experiment commit 与 workflow 的耦合

`experiment commit` 后，`cli.rs` 将 workflow phase 设为 `PendingImplement`——模型已更新但代码未跟上，agent 必须实现后 `sync`。这是两个独立状态变量唯一的耦合点（见 [06-workflow-layer.md](06-workflow-layer.md)）。

## 设计约束

- 轻量级：`start` 只 BFS 加载子图（cap 500），不复制整个 DB。
- 跨 session：JSON 序列化，不依赖 `VACUUM INTO`（那是 backup 的职责）。
- 模拟与执行分离：`evaluate` 用 `SemanticSpace::simulate_retract`（纯内存）；`commit` 用 `CascadeEngine::retract`（真实 DB）。
- Experiment 与 workflow 并行：除 commit→PendingImplement 外，不影响 workflow 顶级状态。
