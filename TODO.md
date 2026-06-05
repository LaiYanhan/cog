# cog TODO

## 接口逻辑简化 ✅

Implemented: `WorkflowState` state machine (`workflow/state.rs`, persisted to `.cog/workflow_state.json`) + `cog next`/`start-change`/`finish-change`/`abort-change` commands + suggestion engine (`workflow/suggestions.rs`).

原来的问题：目前CLI接口逻辑非常复杂，需要LLM充分理解之后才能够发挥相关作用，但这是困难的。因此考虑使用一种状态机的方式，让 CLI 内置 best-practice 从而大幅简化使用难度。

这样就无需 agent 去判断当前究竟应该如何去调用接口，而是直接询问 CLI 就可以得到其下一步可以进行什么操作。

## 推理介质改进（核心方向）

### 方向一：将分支进化为推理沙箱
- [ ] 支持在分支中执行 `impact` 命令，对比主模型与分支模型的差异
- [ ] 支持在分支中执行 `verify`，检查假设性知识与主模型是否矛盾
- [ ] `branch diff` 输出增加受影响的断言数量和下游影响范围摘要
- [ ] 设计 `branch experiment` 子命令：创建分支 → 断言假设 → 自动 impact/verify → 输出风险评估

### 方向二：从"查实体"进化到"问问题"
- [ ] 实现 `cog plan <entity>`：综合 impact 范围 + fragility 密度 + 下游断言状态，输出"改 X 安全吗"的评估
- [ ] 实现 `cog history <entity>`：过滤 correction assertions + changelog，输出实体的事件时间线
- [ ] 实现 `cog path <entity-a> <entity-b>`：双向 BFS 搜索两个实体间的最短路径
- [ ] 实现 `cog changes [--since <date>]`：时间范围内的断言变更摘要（新增、撤销、纠正）

## 实体元数据丰富化 ✅

Implemented: `EntityMetrics` struct (`domain/metrics.rs`) with `line_count`, `fan_in`/`fan_out` (BFS-computed), `visibility`, and `RiskLevel` heuristic. Metrics are populated during tree-sitter scanning and stored in the entities table. `cog query` and `cog index` display and sort by these metrics.

### 已完成项
- [x] tree-sitter 扫描时提取 `line_count`（节点范围差）存入 entities 表
- [x] tree-sitter 扫描时提取 `visibility`（pub/private/crate）存入 entities 表
- [x] 扫描后计算 `fan_in`（入边计数）和 `fan_out`（出边计数）存入 entities 表
- [x] `cog query` 输出展示这些度量
- [x] `cog index` 支持按 line_count / fan_in / fan_out 排序
- [x] schema migration：entities 表增加 line_count、visibility、fan_in、fan_out 列

## 实验层 (Experiment Layer) ✅

Implemented: full hypothesis-testing sandbox (`src/experiment/`) that operates on an in-memory snapshot without modifying the real model. BFS subgraph loading from a focus entity (configurable `max_nodes`), hypothetical assertion ops, cascade simulation and contradiction detection via `evaluate`, plus commit/discard lifecycle. Persisted to `.cog/experiments/<id>.json`.

Commands: `cog experiment start|hypothesize|evaluate|report|commit|discard|list|save|load`.

Integrated into the workflow suggestion engine: `StartExperiment` and `StartExperimentDuringChange` action kinds suggest experiments at appropriate phases.
## 数据模型注意事项

- 所有 schema 变更必须是增量的（只加列/表，不删不改）
- 迁移前必须 `branch create --name pre-migration` 快照
- UUID 稳定性不可破坏
