# 第 5 层：认知潜空间 — `src/space/`

> 图算法层。从 Repository 加载子图到内存，做纯计算的遍历、模拟、风险评估。两个子空间共享 entity 作锚点，但数据类型与操作截然不同。**模拟与执行分离**是本层的核心设计。

## 结构

```
src/space/
├── structure.rs   # StructureSpace（结构子空间，自动层）
├── semantic.rs    # SemanticSpace（语义子空间，TMS 信念系统）
├── cascade.rs     # CascadeEngine（两阶段：模拟验证 + 真实级联）
├── impact.rs      # ImpactEngine（BFS 影响面 + 风险集成）
└── trace.rs       # TraceEngine（DFS 依赖链追溯）
```

`RiskAssessment` 类型定义在 `src/domain/risk.rs`（领域层），供 impact 与 experiment 共享。

## StructureSpace（结构子空间）

```rust
pub struct StructureSpace {
    pub entities: HashMap<String, EntityNode>,  // entity_id → 节点 + 邻接
    pub edges: Vec<StructureEdge>,              // (from, to, kind)
    pub boundary_count: usize,                  // 子图边界未加载的实体数
}
```

只读视图，从 Repository 加载子图，纯内存操作，不持有数据库连接。

**`StructureSpace::load(repo, focus, max_depth, max_nodes)`**：以 `focus` 为中心，沿 entity relations 做 BFS，扩展到 `max_depth` 跳或 `max_nodes` 实体（先到先止）。默认 `max_depth=3`（`0` 表示不限）、`max_nodes=500`。一次性加载全部 `entity_relations` 构建邻接索引（避免 N+1 查询）。到达 cap 时记录 `boundary_count`——这是 experiment 与 impact 输出中"边界实体数"的来源。

`dependents_of_kind(entity_id, kind)`：查询特定 kind 的入边邻居。

## SemanticSpace（语义子空间，TMS）

```rust
pub struct SemanticSpace {
    pub assertions: HashMap<String, AssertionNode>,  // assertion_id → 断言 + evidence
    pub evidence: HashMap<String, EvidenceNode>,
    pub depends_on: Vec<(String, String)>,           // (dependent_id, dependency_id) 边
}
```

**`load(repo, entity_id)`**：加载该 entity 及其 1-hop 相关 entity 的 assertions + evidence + assertion 间的 `depends_on` 边。

**`simulate_retract(assertion_id)`**：**纯内存模拟**——沿反向 `depends_on` 边 BFS，返回 `CascadeReport` 描述"如果撤回会发生什么"，**不触碰真实 DB**。供 experiment 的 `evaluate()` 使用。

**`assess_risk(entity_id, entity_name, structure)`**：综合 fan-in（来自 structure）、active assertions 数、fragility/invariant 数、下游依赖数，计算 `RiskAssessment { risk_score: f64, ... }`。供 `ImpactEngine` 与 experiment 共享。

## CascadeEngine（级联执行）

```rust
pub fn retract(repo: &dyn Repository, assertion_id, reason) -> Result<CascadeReport>
```

**两阶段设计**：

1. **Phase 1**：`SemanticSpace::load(repo, entity_id)` 验证 assertion 上下文。
2. 撤回目标 assertion（`retract_assertion` + changelog）。
3. **Phase 2**：`apply_cascade(repo, retracted_id)` BFS 真实执行级联。

**为何不直接用 `simulate_retract` 的结果**：级联的"是否有独立 Active 依赖"检查需要**实时**查询 Repository 全局状态——加载的局部子图不完整，纯内存模拟可能误判。所以 Phase 2 在真实 DB 上逐节点 BFS，每步实时查 `get_dependents`/`get_dependencies`。

**级联的两种结果**（`CascadeReason`）：

| 原因 | 条件 | 存储状态变更 | 说明 |
|------|------|-------------|------|
| `GroundWeakened` | 该 dependent 还有其它独立 Active 依赖 | **无**（保持 Active） | 只在报告中标记"支撑被削弱" |
| `MarkedUncertain` | 该 dependent 的所有依赖都非 Active | **设为 `Uncertain`** + changelog | 继续向其下游 BFS |

> 这是 TMS 的精确语义。`Uncertain` 的断言可由 `cog recover --apply` 在依赖全部恢复 Active 后还原。完整算法见 [reference/03-tms-cascade.md](../reference/03-tms-cascade.md)。

## ImpactEngine（影响面）

```rust
pub fn analyze(repo: &dyn Repository, entity_name) -> Result<ImpactCard>
```

1. `resolve_entity` 定位实体。
2. `StructureSpace::load(repo, entity, 0, 500)` 加载子图（深度不限，cap 500）。
3. BFS **只跟随 `Calls`+`Uses` 反向边**（谁依赖我）。`Contains` 是结构性而非依赖，排除。
4. 区分直接下游与间接下游，收集受影响的 active assertions。
5. `SemanticSpace::load` + `assess_risk` 计算风险评分。
6. 计算每下游实体的 assertion 数，以及 `downstream_coverage`（有 assertion 的占比）与 `blind_downstream`（无 assertion 的盲区数）。

输出 `ImpactCard`——把"4 个实体依赖你"转化为"哪些下游是盲区、改它风险多大"。

## TraceEngine（依赖链追溯）

```rust
pub fn analyze(repo: &dyn Repository, entity_name) -> Result<TraceTree>
```

DFS 构建 `TraceTree`：entity 的 assertions + 每条 assertion 的 evidence + 递归的 `depends_on` 依赖链（带 visited 集合防环）。用于根因分析——"这个 bug 的根因在哪条推理链上"。

## 设计约束

- **模拟与执行分离**：`SemanticSpace::simulate_retract`（纯内存，experiment 用）vs `CascadeEngine::retract`（真实 DB，CLI `retract` 用）。两者共享 TMS 语义，但作用域不同。
- 图算法用方法（在 Space 类型上）而非自由函数——`space.simulate_retract(id)` 比自由函数更清晰表达操作语义。
- 子图加载有 cap（默认 500），`boundary_count` 如实报告边界——避免假装加载了全部数据。
