# 第 4 层：工作流引导 — `src/workflow/` + `src/command/next_cmd.rs`

> CLI 内嵌的最佳实践。每个命令执行时隐式更新工作流状态，`cog next` 读取状态 + 模型数据 + 活跃实验，给出"下一步做什么"的建议。这是 [concepts/02-descent-protocol.md](../concepts/02-descent-protocol.md) 的"单一引导点"原则的实现。

## 结构

```
src/workflow/
├── state.rs        # WorkflowState + WorkflowPhase + 转换规则
└── suggestions.rs  # SuggestedAction + ActionKind + suggest_actions()
```

`src/command/next_cmd.rs` 是 `cog next` 命令入口：聚合 workflow state + stats + 活跃实验 → 调 `suggest_actions` → 渲染 `NextReport`。

## WorkflowState

```rust
pub enum WorkflowState {
    Uninit,
    Ready { phase: WorkflowPhase },
}

pub enum WorkflowPhase {
    FreshScan,         // 刚 sync，还没有 assertion
    Exploring,         // 浏览、查询、记录断言——所有模型交互
    PendingImplement,  // experiment 已 commit，模型已更新但代码未跟上
    PostChange,        // sync 检测到代码 drift，等待模型对账
    Debugging,         // retract 触发 TMS 级联，或 verify 发现不一致
}
```

序列化到 `.cog/workflow_state.json`，每次命令调用加载、更新、写回。

> **重要纠正**：归档文档曾描述 `Changing` 顶级状态与 `Assessing` 相位，以及 `start-change`/`finish-change`/`abort-change` 命令。**这些已全部移除**。当前只有 `Uninit` 与 `Ready { phase }` 两个顶级状态，五个相位（含新增的 `PendingImplement`）。状态转换是隐式且自动的，无手动状态命令。

## 转换规则

实际转换逻辑在 `state.rs`，由 `cli.rs` 在每条命令后调用：

| 触发命令 | 转换函数 | 效果 |
|---------|---------|------|
| `sync --init`（首次） | `transition_init` | `Uninit` → `Ready{FreshScan}` |
| `sync`（检测到 drift） | `transition_sync(true)` | 任何 `Ready` → `PostChange`；`PendingImplement` → `PostChange` |
| `sync`（无 drift） | `transition_sync(false)` | `PendingImplement` → `Exploring`；其余不变 |
| `assert`/`depend`/`query` | `transition_explore` | `FreshScan`/`PostChange` → `Exploring`；其余不变 |
| `retract` | `transition_retract` | 任何 `Ready` → `Debugging` |
| `verify`（通过） | `transition_verify(true)` | `Debugging` → `Exploring` |
| `verify`（未通过） | `transition_verify(false)` | 保持 `Debugging` |
| `index`/`stats`/`export` | `transition_browse` | 无变化（浏览不改状态） |
| `experiment commit` | cli 直接设 | `Ready` → `PendingImplement` |
| `recover --apply` | `transition_explore` | → `Exploring` |

关键设计：

- **只有 `verify` 通过才能退出 `Debugging`**——浏览（index/stats/export）不算修好。
- **`PendingImplement` 是模型-代码 gap 的追踪**：experiment commit 更新了模型但代码还没改，agent 必须实现后 `sync`。`sync` 检测到 drift → `PostChange`（结构变更需复核）；无 drift → `Exploring`（同步完成）。
- 转换是**容错的**：`save` 失败只打 warning，不阻断命令输出。

## 建议引擎（两阶段决策）

`suggest_actions(state, repo, active_experiments)` 返回 `Vec<SuggestedAction>`。这是确定性的规则引擎，**无 ML、无启发式权重**：

### 阶段 1：实验优先级（phase-independent）

先催促 agent 完成已开始的实验：

- 有 **draft** 实验 → "N draft experiment(s) pending evaluation. Finish before starting new work."
- 有 **evaluated** 实验 → "N evaluated experiment(s) ready. Commit to proceed or discard."

### 阶段 2：phase 特定建议

| Phase | 建议 |
|-------|------|
| `FreshScan` | "Start recording assertions"（**不**建议"N entities 无 assertion"——大代码库会误导） |
| `Exploring` | 覆盖率引导：>80% 建议 verify+实现；>60% 建议 sandbox experiment；低覆盖不催。+ 对最近 assert 的实体建议 `impact` 与深化（invariant/fragility） |
| `PostChange` | 记录 correction + `verify --scan` + 若有 uncertain 则 `recover` |
| `PendingImplement` | "Implement the corresponding code changes now" + 实现后 `sync` |
| `Debugging` | 基于 changelog 的上下文恢复（最近 correction 涉及的实体）+ 审查 retracted/uncertain + 记录违反的约束为 invariant |

### 阶段 3：停滞检测（`detect_stagnation`）

三条规则（窗口 `STAGNATION_WINDOW = 5`）：

| 规则 | 触发条件 | 建议 |
|------|---------|------|
| Verify 循环 | 最近 5 条 changelog 全为 `Verify` | "Consider implementing rather than further analysis" |
| Stale 实验 | 某 Evaluated 实验的文件 mtime 之后累积 ≥5 条 changelog | 该实验被遗忘，催促 commit/discard |
| Guard | 最近 5 条全为 `Assert` | **不触发**（密集建模是进展信号） |

> 停滞检测只看**写操作** changelog，不看只读操作——所以"反复 query 但不 assert"的瘫痪无法靠此检测（避免为只读操作写 changelog 导致 DB 膨胀）。

## ActionKind（14 种）

`InitProject`、`StartRecording`、`RecordMissingContracts`、`ReviewUncertainAssertions`、`AssessImpact`、`RecordFix`、`VerifyConsistency`、`StartExperiment`、`SyncModel`、`ImplementNow`、`CommitExperiment`、`RecoverContext`、`RecordConstraint`、`ImplementPlanned`。

阈值常量（`suggestions.rs`）：`STAGNATION_WINDOW=5`、`COVERAGE_IMPLEMENT_THRESHOLD=60.0`、`COVERAGE_REFINE_THRESHOLD=80.0`。

## 两个独立状态变量

工作流状态（代码空间）与 experiment 状态（潜空间）是**独立的**——见 [concepts/02-descent-protocol.md](../concepts/02-descent-protocol.md) §5。`cog next` 读取两者，通过上述两阶段决策合成建议，而非笛卡尔积。`next_cmd::detect_active_experiments` 扫描 `.cog/experiments/` 目录，按 mtime 排序返回活跃实验列表。

## 设计约束

- CLI 每次调用是新进程，typestate 的编译期保证无法跨进程传递——用序列化 enum + 运行时建议引擎。
- 建议引擎是纯函数式的：`(state, repo, experiments) → Vec<SuggestedAction>`，无副作用，便于测试。
- 建议避免"一刀切"——例如 FreshScan 不建议 blanket-asserting 大代码库的所有实体，而是聚焦当前任务。
