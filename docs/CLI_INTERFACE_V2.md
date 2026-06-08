# Cog CLI 接口设计 V2

> 日期：2026-06-08
> 前置阅读：`docs/COGNITIVE_MODEL_DESIGN.md`, `docs/RUST_ARCHITECTURE_REDESIGN.md`, `docs/LATENT_TO_CODE_MAPPING.md`
> 基于：SWE-CI benchmark 10-task pilot 轨迹分析 + 下降协议设计


# 偏差说明：设计 vs 实现

本章记录了 V2 设计方案与实际代码实现之间的偏差决策。一个设计文档的价值不仅在于它说了什么，更在于哪些部分被证明不切实际——为未来的迭代者省去重蹈覆辙的成本。

## 未实现的设计

### `next --verbose` 合并 stats（§4.1，§7.2）

**设计**：`stats` 的详细统计内容合并到 `next --verbose`，独立命令保留用于脚本。

**未实现的理由**：当前 `next` 输出已包含对 LLM 决策有实质价值的所有统计维度（entities、assertions、active/retracted 分布、coverage）。剩余的 `evidences`、`corrections`、`uncertain_assertions` 字段对每 epoch 的认知循环没有影响——不确定性已在 Debugging 状态建议中体现，详情只在人工调试时需要。保留 `stats` 作为独立命令，`NextArgs` 保持空结构体。

### `impact --verbose` 展开下游详情（§4.4）

**设计**：`cog impact <entity> --verbose` 展开每个 downstream entity 的断言状态。

**未实现的理由**：当前默认输出已覆盖每 entity 的 covered/blind 标记（含 assertion 数量）、risk score、WARNING。LLM 需要更深信息时应直接 `cog query <entity>`——这正是 impact 输出底部的 `Next:` 建议引导的路径。添加 `--verbose` 会创造一条几乎不会走的代码路径，徒增格式化分支。

### `[COG:<command>]` 输出前缀（§6.2）

**设计**：每条命令输出第一行以 `[COG:impact]` 等前缀标识类型。

**未实现的理由**：每种输出的首行格式天然不同（impact→`Impact for: X [type]`，next→`State: Ready/Exploring`，query→`<entity> [kind]`，assert→`Created <id>`）。前缀对 LLM 不提供额外信息（LLM 已知它调用的是哪个命令），反而增加每条输出 ~12 token 的固定开销（14 epoch × 5 命令 = ~840 token）并在 `Renderable` trait 与命令层之间引入不必要的耦合。

### 停滞检测扩展——检测 query/impact 只读循环（§4.1）

**设计**：停滞检测应在最近 5 条 changelog 全为只读操作（query/impact/trace/index/stats）时触发。

**未实现的理由**：changelog 仅记录写操作（Assert、Retract、Depend、Sync、Verify），不记录只读操作。为检测"agent 反复 query 但不 assert"需要每次只读操作写 changelog——这意味着 DB 写入量增加 2-5 倍/epoch，changelog 表膨胀，Repository trait 需要新 `ChangelogAction` 变体。SWE-CI 数据显示真正的停滞是 verify loop 和 experiment stale——这两者已能检测。当前三层规则（all-verify 停滞 ✓、experiment stale by mtime ✓、密集 assert 不误触发 ✓）覆盖了 pilot 数据中所有关键模式。

### `index --top` flag（§4.8）

**设计**：`cog index --top 10` 按 assertion 数量展示 top-N。

**未实现的理由**：LLM 需要知道的是"哪里没覆盖"和"哪些盲区影响面大"。前者由 `--uncovered` 解决，后者是 `cog impact` 的职责。`--top` 会模糊 index（覆盖概览）和 impact（影响面分析）的职责边界，增加一个与现有命令重叠的 flag。

### `sync --clean` flag（§4.2）

**设计**：`cog sync --clean` 同时清理孤立 entity。

**未实现的理由**：stale entity（源码已删除）和孤立 entity（无任何 relation）是两个正交概念。sync 的职责是检测代码空间 drift——`--clean` 本意清理孤立 entity 但实际只删 stale，与 `--update` 模式重复。孤立清理是 `verify --clean` 的职责。移除此 flag 使两个命令的职责更清晰：sync→drift，verify→consistency。

## 设计中未充分说明的地方（实现时补充决策）

### `sync` 命令设计（§4.2，§7.3）——init 合并为幂等 sync

**设计**：`init` 只跑一次，后续用 `sync --update`。§7.3 描述了两步流程：`sync` 仅检测 drift，`sync --update` 应用变更。

**为何偏离**：两个独立命令引入认知负担（agent 需知道"何时用 init 何时用 sync"），且 `sync --update` 不创建关系会导致模型半同步状态（新 entity 无 contains/calls/uses 关系，`cog impact` 返回误导性的 0 downstream）。实现发现 init 和 sync 跑的是**同一个 AST 扫描**（都为 `Scanner::scan()`），`upsert_entity` 已是幂等的——两者区别纯属 CLI 层面的区分。合并为单一 `cog sync` 命令（幂等、全量、创建关系、stale 安全清理）消除了半同步风险，且代码量减少（删掉重复的 sync 逻辑）。§7.3 中的两步协议规范已更新为幂等模型，原 `sync --update` / `sync --clean` 不再存在。

### `cog next` 合成逻辑（§7.2，§7.6，LATENT §7.3）——从笛卡尔积到两阶段决策

**设计**：`ExperimentStatus`（None/Draft/Evaluated）与 `WorkflowPhase`（FreshScan/Exploring/PostChange/Debugging）做笛卡尔积，每个 `(phase, status)` 组合对应一条建议。`detect_experiment_status()` 返回 `Option<String>`，只报告第一个活跃实验。

**为何偏离**：

1. **多并行实验使笛卡尔积爆炸**。当允许多个并行活跃实验时，`ExperimentStatus` 不再是 `None/Draft/Evaluated` 三值，而是 `Vec<ActiveExperiment>`。组合数从 ~12 膨胀到 15+，且无法清晰表达"N 个 draft + M 个 evaluated"的建议语义。

2. **两阶段决策更清晰**。实现将建议逻辑重构为：
   - **阶段 1**：实验优先级（phase-independent）——draft > evaluated，先催促 agent 完成已开始的实验
   - **阶段 2**：阶段特定建议——按 WorkflowPhase 给出标准建议
   - **阶段 3**：停滞检测——verify 循环 / experiment stale

   实验状态和 workflow phase 不再耦合为排列组合，而是独立贡献建议。`suggest_for_ready()` 的代码行数从 ~180 行降到 ~120 行，且自动支持多并行实验。

3. **`detect_experiment_status()` → `detect_active_experiments()`**。返回类型从 `Option<String>` 改为 `Vec<ActiveExperiment>`。`ActiveExperiment` 包含 `short_id`、`description`、`status`、`mtime`（文件修改时间）。`NextReport` 中 `experiment_status: Option<String>` 字段替换为 `active_experiments: Vec<ActiveExperiment>`。

### 停滞检测设计（§4.1）——实验 stale 检测机制

**设计**：原文描述"若检测到活跃 ExperimentStatus::Evaluated 且超过 N 次命令调用未 commit/discard，亦触发停滞建议"，但未定义"N 次命令"如何计数。

**实现决策**：使用实验 JSON 文件的 **mtime**（修改时间）作为评估时间代理。在 `detect_stagnation()` 中，对每个 Evaluated 实验，计算其文件 mtime 之后的 changelog 条目数。如果 >= `STAGNATION_WINDOW`（5），触发 stale 警告。选择 mtime 而非在 experiment 结构中添加 `evaluated_at` 字段的理由：

- 不需要修改 experiment 序列化格式（向后兼容）
- `mark_evaluated()` + `save()` 的调用总是更新文件 mtime，天然准确
- 无需为已有实验文件写迁移逻辑

当前停滞检测的三条规则：

| 规则 | 触发条件 | 检测方法 |
|------|---------|---------|
| Verify 循环 | 最近 5 条 changelog 全为 Verify | changelog 类型检查 |
| Stale 实验 | Evaluated 实验后累积 >= 5 条 changelog | 文件 mtime vs changelog timestamp |
| Guard | 最近 5 条 changelog 全为 Assert | 不触发（密集建模是进展信号） |

### `sync` drift 检测机制（§7.3）——从字符串匹配到结构化字段

**设计**：原文未明确 sync dispatch 如何判断 drift——实现中曾用 `!out.text.contains("no drift")` 从格式化输出文本推断。

**为何偏离**：输出文本是面向 agent 的可读格式，不应作为状态机转换的信号源。格式化文本变化（如改为 "Model is up to date"）会导致状态转换悄悄失效。实现改为在 `CommandOutput` 中添加 `has_drift: bool` 字段，`sync_cmd::execute()` 根据内部 `SyncReport.has_drift` 设置此字段，CLI dispatch 直接读取 `out.has_drift`。`CommandOutput` 的构造函数将 `has_drift` 默认为 `false`，其他命令不受影响。

### `retract` 输出中 retracted assertions 的处理（§4.6）

**设计**：§4.6 要求 retract 后列出 entity 的"剩余 assertions"，标记状态变化为 `[uncertain]`。

**实现补充**：原实现将 `get_assertions_for_entity()` 返回的全部 assertions（含刚 retract 的那条）传入渲染器。`!is_active()` 同时匹配 `Retracted` 和 `Uncertain`，导致 retracted assertion 被误标为 `[uncertain]` 出现在 "now has N active assertions" 列表中。修复：在 `retract.rs` 中过滤 `AssertionStatus::Retracted`，只传入 Active + Uncertain 给渲染器。这样 `!is_active()` 只匹配 `Uncertain`，标记语义正确。

### `experiment try` 支持 `--depends-on`（§4.7）

**设计**：§4.7 的 `experiment try` 输入签名未列出 `--depends-on`。`try` 定位为"快速一阶推演"（80% 场景），复杂场景走分步路径。

**实现补充**：TMS cascade 检测依赖于 `depends_on` 链——不传 `--depends-on` 时 `evaluate` 的 cascade_count 始终为 0，contradiction 检测缺少关键维度。添加 `--depends-on` 为可选参数，`ExperimentOp::Assertion` 已有此字段，改动仅涉及 CLI 层（`Try` variant 签名 + dispatch + `try_experiment` 参数传递），无需修改 experiment 核心逻辑。

---

# 第一部分：设计哲学与方法

## 1. 四条设计原则

### 原则 1：注意力经济（Economy of Attention）

> Cog 的每一条输出都在与代码本身竞争 agent 的 context window。

| 规则 | 说明 |
|------|------|
| **默认精简** | 默认输出只包含当前认知操作最需要的信息。不是"信息越多越好"，是"恰好足够" |
| **按需展开** | 细节通过 flag（`--verbose`, `--compact`）获取。flag 本身也是信息——告诉 agent "这里可以深入" |
| **token 预算意识** | 下表中的操作是每 epoch 的固定开销，累计必须控制 |

| 操作 | 估算 token | 频率（per epoch） | 累计（14 epoch 任务） |
|------|-----------|------------------|---------------------|
| `cog next` | 150-250 | 1x | 2,800-3,500 |
| `cog query` (per entity) | 100-200 | 2-5x | 2,800-14,000 |
| `cog impact` (per entity) | 200-400 | 1-3x | 2,800-16,800 |
| `cog assert` (per assertion) | 80-150 | 1-5x | 1,120-10,500 |
| `cog retract` (per retraction) | 80-200 | 0-2x | 0-5,600 |
| **总计（14 epoch）** | — | — | **10K-50K tokens** |

SWE-CI 中 9001__co 的实际消耗远超此表——因为 agent 额外跑了 `verify --scan` 和 `stats` 每个 epoch（当前设计中这些是冗余的），且 query 结果冗长。可接受的上限是 50K，当前实现约为 80K+。

### 原则 2：即时上下文（Immediate Context）

> 外挂认知模型应与人类程序员的认知方式对齐。人类写完注释会自然扫一眼已有注释——这是即时的、无需额外操作的。

**写操作必须返回受影响 entity 的当前状态**：

| 操作 | 输出包含 |
|------|---------|
| `assert` | 操作确认 + 该 entity 的**所有 active assertions**（新创建者高亮） |
| `retract` | 操作确认 + **级联影响** + 该 entity 的**剩余 assertions**（状态变化标记） |
| `depend` | 操作确认 + 该 entity 的**所有关系**（新增 + 已有） |

这个原则不仅是效率优化，更是下降协议 Phase 4/5（Probe/Complete）中快速 feedback loop 的基础。agent 修改模型后不需要 extra query 来"回头看"——输出本身就包含了后续决策所需的状态。

### 原则 3：下降感知（Descent-Aware Design）

> 每个命令都应有明确的"下降阶段定位"——它在从潜空间到代码空间的哪个 checkpoint 起作用。

| 下降阶段 | 主导命令 | 输出特征 |
|---------|---------|---------|
| Phase 1: SURVEY | `next`, `query`, `impact`, `sync` | 提供理解，不做判断 |
| Phase 2: HYPOTHESIZE | `experiment` | 沙盘推演，检测矛盾 |
| Phase 3: SCOUT | `query`（读实际代码） | 验证假设与现实 |
| Phase 4: PROBE | `assert`, `retract`（快速记录发现） | 即时反馈，紧循环 |
| Phase 5: COMPLETE | `assert`, `retract`, `sync` | 完整更新模型 |

关键设计决策：`experiment` 是下降协议的核心通路，不是可选功能。在 SWE-CI prompt 中它被放在 "3+ downstream 时才用" 的子分支里——这是错误的定位。`experiment` 应作为任何非 trivial 修改的默认第一步，`cog next` 应据此建议。

### 原则 4：单一引导点（Single Point of Guidance）

> Agent 不需要知道 18 个命令的用法。它只需要知道一个命令：`cog next`。

`cog next` 是 agent 与 cog 交互的**唯一入口**。所有其他命令都是由 `next` 的建议引出的下行路径，不是 agent 需要主动记忆的菜单项。

```
cog next
  - "182 entities lack assertions" > cog query <core_entity> > cog assert ...
  - "Assess impact before changes" > cog impact <entity>
  - "Stagnation detected" > cog experiment try <entity>
  - "Model needs sync" > cog sync --update
  - "Good coverage. Start implementing" > (agent moves to code changes)
```

这意味着 `next` 的输出是 agent 决策的**结构化输入**——不是自由文本段落，而是可执行的建议列表。

---

## 2. 接口三大支柱

Cog 的 CLI 接口围绕三个功能维度组织。这不是技术分层（那是 Rust 架构的事），而是**agent 视角下的认知操作分类**。

### 支柱 1：入口（Entry）——信息如何流入模型

> 将代码空间和人类理解转换为潜空间中的结构化知识。

| 命令 | 输入来源 | 产物 |
|------|---------|------|
| `sync` | 代码空间（tree-sitter 扫描） | entity graph + relations（自动层，幂等可重复） |
| `assert` | agent 理解 | assertion（语义层） |
| `retract` | agent 修正 | assertion status 变更 + TMS 级联 |
| `depend` | agent 判断 | entity relation（调用/包含/使用） |

**设计目标**：写操作必须**低摩擦**。agent 记录一个认知判断的 token 开销应接近写一个注释——而不是走一个多步骤的仪式化流程。

### 支柱 2：出口（Exit）——信息如何流出模型

> 将潜空间中的结构化知识转换为 agent 可执行的决策依据。

| 命令 | 输出类型 | 指导的决策 |
|------|---------|-----------|
| `next` | 状态摘要 + 建议列表 | "我现在该做什么？" |
| `query` | entity 认知卡片 | "我对这个 entity 了解了什么？" |
| `impact` | 影响面 + 风险评估 | "改它安全吗？需要注意什么？" |
| `trace` | 依赖链追溯 | "这个 bug 的根因在哪条推理链上？" |
| `index` | 覆盖摘要 | "哪些模块还需要建模？" |

**设计目标**：输出必须**可执行**。不是返回数据列表，而是回答 agent 真正想问的问题。`impact` 不应回答"4 个 entity 依赖你"——它应回答"改它安全吗？哪些下游是盲区？"

### 支柱 3：映射（Mapping）——潜空间推理如何转换为代码变更

> 下降协议的 CLI 支持层。这是当前 cog 缺失的功能维度。

| 机制 | CLI 支持 | 作用 |
|------|---------|------|
| 沙盘推演 | `experiment try` | Phase 2：在潜空间中推演变更计划 |
| Scout 指引 | `experiment evaluate` 输出中的 "Entities to scout" | Phase 3：引导 agent 去验证关键假设 |
| Probe 循环 | `assert`/`retract` 即时反馈 | Phase 4：实现过程中快速更新模型 |
| 失败回流 | `assert --kind fragility` + experiment discard | Phase 4/5：记录失败教训到模型 |
| 变更确认 | `experiment commit` + `sync` | Phase 5：完成变更，同步模型 |

**设计目标**：下降协议不需要新的 CLI 命令。它通过在现有命令的输出中嵌入**下降阶段信号**（scout 建议、checkpoint 确认、失败处理指引）来实现。

---

## 3. 命令频率分层

按照下降协议中每个命令的实际使用频率，将命令分为三层：

### 高频层（每 epoch 1-N 次）

这些命令构成核心认知循环。它们的延迟和 token 开销直接影响 agent 的工作效率。

| 命令 | 下降阶段 | 每 epoch 调用次数（SWE-CI 数据） | 设计约束 |
|------|---------|-------------------------------|---------|
| **`next`** | Phase 1 入口 | 1x | 输出 ≤ 300 tokens；合并 stats；含停滞检测 |
| **`query`** | Phase 1/3 | 2-5x | 默认模式 ≤ 200 tokens；`--compact` ≤ 100 tokens |
| **`impact`** | Phase 1 | 1-3x | 默认模式 ≤ 300 tokens；risk description 可执行 |
| **`assert`** | Phase 4/5 | 1-5x | 输出包含 entity 完整状态（≤ 300 tokens） |
| **`retract`** | Phase 4/5 | 0-2x | 同上，含级联列表 |

### 中频层（epoch 边界）

| 命令 | 下降阶段 | 每 epoch 调用次数 | 设计约束 |
|------|---------|-------------------|---------|
| **`sync`** | Phase 1 | 0-1x | 快速完成（纯 I/O，无 LLM 交互） |
| **`experiment`** | Phase 2-5 | 0-1x | `try` 子命令一行完成；`evaluate` 含 scout 指引 |

### 低频层（全生命周期几次）

| 命令 | 用途 | 设计约束 |
|------|------|---------|
| `sync` | 代码空间同步（idempotent） | 已合并 init 功能 — 可重复执行，创建 entity + relation + 清理 stale |
| `trace` | 根因追溯 | 不修改 |
| `index` | 覆盖浏览 | 默认摘要模式 |
| `depend` | 手动关系 | 不修改（保持现有行为） |
| `export` | 模型导出 | 不修改 |
| `backup` | 全量备份 | 不修改 |
| `stats` | 详细统计 | 内容合并到 `next --verbose`，独立命令保留用于脚本/调试 |
| `delete-entity` | 实体删除 | 不修改 |

---

# 第二部分：具体设计方案

## 4. 命令逐一设计

后续命令设计中，格式规范使用以下约定：
- `<placeholders>` 表示由用户填入的值
- `Next:` 建议的下一个可执行命令
- `WARNING:` 表示警告声明
- `[status]` 表示 assertion 状态标记
- `[new]` 表示新创建元素

### 4.1 `cog next` — 单一引导点

**定位**：下降协议 Phase 1 入口。agent 每个 epoch 应首先执行此命令。

**合并**：替代当前 `cog next` + `cog stats` + `cog verify` 的三命令开幕。

**输入**：
```
cog next                        # 基础模式：状态 + 建议
cog next --verbose              # 展开 stats 详细信息
```

**输出（基础模式）**：
```
State: Ready/Exploring
Experiment: none
Model: 650 entities, 57 assertions (45 active, 12 retracted)
Coverage: 72% (468/650)

Suggestions:
  1. [assess] 3 entities in the change path lack assertions.
     Next: cog impact <entity> to see blast radius before modifying.
  2. [model] 182 entities still uncovered. Focus on core modules.
     Next: cog assert <entity> --kind contract --claim "..." --grounds "code:<entity>"
  3. [drift] No sync in 8 operations. Model may be stale.
     Next: cog sync
  4. [descent] Consider a sandbox experiment before implementing.
     Next: cog experiment try <entity> --kind correction --claim "..." --grounds "code:<entity>" --desc "..."

Status: OK
```


**停滞检测触发时的额外行**：
```
WARNING: Model unchanged in recent operations. Consider implementing rather than
  further analysis. The current approach may need a concrete attempt.
  Next: Start with: cog experiment try <target> --kind correction --claim "..." --grounds "code:<target>" --desc "..."
```

**覆盖率自适应**：
- coverage > 60%：不再建议 "record contracts"，改为建议 "start implementing" 或 "experiment try"
- coverage > 80%：建议聚焦于 "verify consistency" 和 "refine existing assertions"

**关键决策**：
- `sync` 不自动执行——`next` 在检测到模型可能 stale 时建议 `cog sync`（而非通过 flag 合并到 next）
- 停滞检测使用两级信号：(1) 查询最近 5 条 changelog，若全为 verify/stats/index/query 等只读操作则触发；(2) 若检测到活跃 ExperimentStatus::Evaluated 且超过 N 次命令调用未 commit/discard，亦触发停滞建议。注意：若最近 5 条 changelog 全是 assert（agent 在密集建模），不应触发停滞——只读操作为停滞信号，建模是进展信号。
- 建议列表中每条建议都包含可直接复制执行的命令

**实现改动**：`workflow/suggestions.rs` 的 `suggest_actions()` 增加覆盖率计算、停滞检测；输出格式更新。

### 4.2 `cog sync` — 模型与代码同步
### 4.2 `cog sync` — 模型与代码幂等同步

**定位**：统一的代码空间→潜空间入口。替代原有的 `cog init`（一次性）和 `cog sync`（增量）两个命令。

**核心语义**：`cog sync .` 执行完整 tree-sitter 扫描，创建/更新 entity **和** 关系，删除不再存在于代码中的 stale entity（但跳过已有 assertion 的 entity——避免数据丢失）。幂等：可随时重复执行。`--dry-run` 仅报告不写入。

**输入**：
```
cog sync .                      # 幂等全量同步（默认）
cog sync . --dry-run             # 仅报告变更，不写入
cog sync src/                    # 限定扫描路径
cog sync . --lang python,rust    # 限定语言
cog sync . --depth 2             # 限定目录深度
```

**输出（首次或有变更）**：
```
Sync: 42 files scanned (python: 30, rust: 12)
  +186 entities created, -0 removed, 234 relations
  module: 42
  function: 89
  type: 55
After sync: 186 entities, 0 assertions
Next: cog index | cog impact <entity>
```

**输出（无变更）**：
```
Sync: 42 files scanned (python: 30, rust: 12)
After sync: 186 entities, 15 assertions
Model is up to date — no drift.
Next: cog index | cog impact <entity>
```

**输出（有 stale 但有 assertion 保护）**：
```
Sync: 42 files scanned (python: 30, rust: 12)
Skipped 1 stale entities (have assertions):
  - old::deprecated_fn  (use `cog delete-entity old::deprecated_fn` to force)
After sync: 186 entities, 15 assertions
Next: cog index | cog impact <entity>
```

**关键决策**：
- 合并 init 的完整扫描和 sync 的 drift 清理——agent 只需记一个命令
- stale entity 删除前检查 assertion——保护 agent 记录的认知知识
- `--dry-run` 替代了旧 `cog sync` 的"仅报告不修改"模式

**实现改动**：原 `command/init_cmd.rs` 和 `command/sync_cmd.rs` 合并为 `command/sync_cmd.rs`。旧 `cog init` 和旧 `cog sync` 移除。
### 4.3 `cog query` — 实体认知卡片

**定位**：下降协议 Phase 1（了解 entity）+ Phase 3（验证假设）。

**输入**：
```
cog query <entity>              # 仅 active assertions（默认）
cog query <entity> --all        # 含 retracted
cog query <entity> --compact    # 单行模式，用于嵌入 impact/trace 上下文
```

**输出（默认）**：
```
SeqBuilder [type]

assertions (3 active, 1 retracted):
  [contract] e5f6a7b8: returns SeqBuilder instance for chaining
    grounds: code:SeqBuilder
  [invariant] c9d0e1f2: always initializes _gen attribute
    grounds: code:SeqBuilder.__init__
  [fragility] g3h4i5j6: relies on generator protocol consistency
    grounds: code:SeqBuilder.__call__

relations (2):
  -> contains Seq [type]
  <- called_by test_builder [module]
```

**输出（`--compact`）**：
```
SeqBuilder [type] — 3 active:
  [contract] e5f6a7b8: returns SeqBuilder instance for chaining
  [invariant] c9d0e1f2: always initializes _gen attribute
  [fragility] g3h4i5j6: relies on generator protocol consistency
```

**关键决策**：
- evidence 折叠到 assertion 行内（`grounds: code:SeqBuilder`），不逐条展开
- `--compact` 模式一行一个 assertion，方便在 context window 有限时使用
- retracted assertions 默认隐藏（`--all` 显示），因为下降协议中 agent 关注的是"当前成立的是什么"

**实现改动**：`format/text.rs` 调整 `QueryCard` 渲染。

### 4.4 `cog impact` — 变更影响面 + 风险信号

**定位**：下降协议 Phase 1 的核心工具。回答 agent 真正想问的问题："改它安全吗？需要注意什么？"

**与现有 doc 设计一致，额外增加**：
- Scout 建议：evaluate 输出列出应在代码中验证的 entity

**输入**：
```
cog impact <entity>             # 默认：风险概览 + downstream 摘要
cog impact <entity> --verbose      # 展开每个 downstream entity 的断言状态
```

**输出（默认）**：
```
Impact for: SeqBuilder [type]

Risk: LOW (0.30)
  Downstream: 4 entities (3 covered, 1 blind)
  Active assertions at stake: 2
  WARNING: Risk reflects structural dependencies only.
    Runtime behavior (generators, metaclasses, dynamic dispatch)
    is NOT captured. Verify blind entities before implementing.

Downstream:
  Seq [type]         covered (1 assertion)
  SeqGen [type]      blind (0 assertions)
  SeqBuilderTest     covered (2 assertions)
  test_generators    covered (1 assertion)

Next: For details on each entity: cog impact <entity> --verbose
Next: Test your plan safely: cog experiment try <entity> --assert ... --desc "..."
```

**关键变化（与现有设计一致 + 新增）**：
- `covered` vs `blind` 二元标记：agent 立刻知道哪些下游需要额外小心
- WARNING: 显式声明局限性——消除 dbrattli 中 "0 downstream = 安全" 的误判
- **新增 scout suggestion**：当存在 blind entity 时，建议 `cog query <blind_entity>` 或 `cog experiment try`
- **新增 experiment 引导**：输出底部提示 agent 可以用 experiment 做沙盘推演

**实现改动**：
- `src/space/risk.rs`：RiskAssessment 增加 `downstream_coverage: f64`, `unmodeled_downstream: usize`
- `src/space/semantic.rs`：`assess_risk()` 增加覆盖率计算；当 entity 有 ≥3 active assertions 但 downstream=0 时，minimum risk = 0.3
- `src/space/impact.rs`：ImpactEngine 为每个 downstream 计算 assertion 状态
- `src/format/text.rs`：渲染新格式 + scout 建议

### 4.5 `cog assert` — 即时上下文写入

**定位**：下降协议 Phase 4/5 的模型更新操作。写一个 assertion 后立即看到 entity 完整状态。

**输入**：
```
cog assert <entity> --kind <kind> --claim "<text>" --grounds "<source>"
cog assert <entity> --kind <kind> --claim "<text>" --grounds "<source>" --depends-on <id>
```

**输出**：
```
Created a1b2c3d4 [contract] on SeqBuilder
  "drives generator via gen.send(value)"

SeqBuilder now has 4 active assertions:
  1. [contract] a1b2c3d4: drives generator via gen.send(value)    [new]
  2. [contract] e5f6a7b8: returns SeqBuilder instance for chaining
  3. [invariant] c9d0e1f2: always initializes _gen attribute
  4. [fragility] g3h4i5j6: relies on generator protocol consistency

WARNING: SeqBuilder already has 1 contract assertion. Ensure this one adds new
  information rather than duplicating.
  Next: To replace: cog retract e5f6a7b8 --reason "superseded by a1b2c3d4"
```

**关键设计（与现有设计一致）**：
- 创建确认后**立即列出 entity 全部 assertions**——人类写注释后看一眼页面，这就是那个"看一眼"
- 新 assertion 用 `[new]` 标记
- **同 kind 检测**：当 entity 已有同 kind assertion 时，提示 agent 自省（**引导，不阻止**——与 cog 的"结构 vs 语义"分工一致）

**实现改动**：`src/command/assert_cmd.rs` 创建 assertion 后额外查询 entity 现有 assertions 并格式化输出；判断同 kind 计数。

### 4.6 `cog retract` — 级联感知撤回

**定位**：下降协议 Phase 4/5。当实现颠覆了先前的认知假设时，撤回 + 看影响范围。

**输入**：
```
cog retract <id> --reason "<why>"
```

**输出**：
```
Retracted e5f6a7b8 [contract] on SeqBuilder
  Reason: "superseded by a1b2c3d4"

Cascade: 1 assertion affected
  g3h4i5j6 [fragility] -> uncertain (ground weakened)
    "relies on generator protocol consistency"

SeqBuilder now has 3 active assertions:
  [contract] a1b2c3d4: drives generator via gen.send(value)
  [invariant] c9d0e1f2: always initializes _gen attribute
  [fragility] g3h4i5j6: relies on generator protocol consistency [uncertain]

Next: g3h4i5j6 is now uncertain. Re-verify it:
    cog query SeqBuilder --all
```

**关键设计**：
- 展示级联影响（依赖于 `--depends-on` 链的存在——这就是为什么 prompt 必须引导 agent 使用 `--depends-on`）
- 展示 retract 后 entity 的剩余 assertions，**标记状态变化**（`[uncertain]`）
- 有 uncertain 时，提示 agent re-verify

**实现改动**：`src/command/retract.rs` 执行 retract 后查询 entity 状态 + cascade 结果；`src/format/text.rs` 渲染。

### 4.7 `cog experiment` — 下降协议的核心通路

**定位**：不是可选功能，是下降协议 Phase 2-5 的承载者。

**核心设计变化**：引入 `experiment try` 作为默认的快速推演入口，覆盖 80% 使用场景；保留原子命令用于复杂场景。关键新增：**evaluate 输出包含 scout 指引**。

**输入**：
```
# 快速推演（覆盖 80% 场景）
cog experiment try <entity> \
    --kind <kind> --claim "<text>" --grounds "<source>" \
    [--desc "what if we change X?"]           # 可选。省略时默认为 "<entity>: <claim>"


# 分步操作（覆盖 20% 场景——需要注入多个 hypothesis 的复杂推演）
cog experiment start <entity> --desc "..."
cog experiment hypothesize <id> --assert ...
cog experiment hypothesize <id> --delete <entity>   # 也支持 delete/relation
cog experiment evaluate <id>
cog experiment commit <id>
cog experiment discard <id>

# 管理
cog experiment list
cog experiment report <id>
```

**输出（`experiment try`）**：
```
Experiment a1b2c3d4: "what if we change SeqBuilder.send() to next()?"

Hypothesis:
  + [correction] SeqBuilder: "drives generator via next() instead of send()"

Evaluation:
  Risk: HIGH (0.82)
  Contradictions: 2
    1. test_builder expects send() behavior (test:test_builder.py:42)
    2. SeqGen contracts assume gen.send() is available
  Affected assertions: 7
  Cascade: 3 assertions -> uncertain

Scout before implementing:
  [read] SeqBuilder [type] — read __call__ implementation
  [assert] SeqGen [type] — blind (0 assertions), verify expected behavior
  [verify] test_builder [module] — check test expectations at line 42

Next: Discard: cog experiment discard a1b2c3d4
Next: Adjust hypothesis: cog experiment hypothesize a1b2c3d4 --kind correction --claim "..." --grounds "code:<entity>"
Next: Proceed to implementation: cog experiment commit a1b2c3d4
```

**Scout 操作标签**：
- `[read]` — 需要阅读源码验证假设
- `[assert]` — blind entity，应优先记录观察结果
- `[verify]` — 有 contradiction 或边界数据不完整，需特别注意

**Scout 指引的生成逻辑**：
- 有 contradictions 的 entity → 必须 scout（标记 `[verify]`）
- blind entity（被推演影响但无 assertion） → 建议 scout（标记 `[assert]`）
- boundary entity（subgraph 边界） → 可选 scout（标记 `[read]`，标注为 partial data）

**成功案例的 evaluate 输出**：
```
Experiment c9d0e1f2: "separate __new__ from __init__ in Signal"

Evaluation:
  Risk: LOW (0.30)
  Contradictions: 0
  Affected assertions: 2
  Cascade: 0

Scout before implementing:
  [read] Signal.__new__ [function] — verify current signature
  Next: Safe to proceed: cog experiment commit c9d0e1f2
```

**关键设计决策**：
- `experiment try` 一行命令覆盖 start + hypothesize + evaluate，零摩擦启动沙盘推演。`--desc` 可选；省略时默认生成 `"<entity> hypothesis: <claim>"`
- **evaluate 输出必须包含 Scout 指引**——这是下降协议 Phase 2→3 的桥梁
- evaluate 在成功场景输出"Safe to proceed: commit"，在失败场景输出"Discard or adjust"
- **BREAKING CHANGE**：experiment 的状态机改为 `Open ↔ Evaluated`（hypothesize 在 Open 下追加，在 Evaluated 下也可追加；evaluate 在 Open 和 Evaluated 下均可执行——支持 Phase 3/4 的 re-evaluate），`Evaluated → Committed/Discarded`。当前代码 `session.rs` 中 `evaluate()` 和 `mark_evaluated()` 仅允许 `Open` 状态，实现时需放开此约束并确保 `mark_evaluated` 在 `Evaluated` 状态下幂等（no-op）。

**实现改动**：
- `src/cli/experiment.rs`：新增 `ExperimentAction::Try`
- `src/command/experiment_cmd.rs`：`try` 子命令合并 start + hypothesize + evaluate
- `src/experiment/report.rs`：`ExperimentReport` 增加 `scout_suggestions: Vec<ScoutSuggestion>`
- `src/experiment/session.rs`：允许 evaluate 在 `Evaluated` 状态下重新执行（支持 Phase 3/4 的 re-evaluate）

### 4.8 `cog index` — 覆盖摘要

**定位**：低频浏览工具。默认输出从全量列表变为覆盖摘要。

**输入**：
```
cog index                        # 默认：覆盖摘要
cog index --uncovered            # 仅无 assertion 的 entity
cog index --top 10               # assertion 最多的 top-N
cog index --prefix <prefix>      # 按模块过滤
cog index --verbose              # 全文列表（恢复旧行为）
cog index --kind function        # 按 entity kind 过滤
```

**输出（默认）**：
```
Coverage: 468/650 (72%)

By module (top 5 uncovered):
  src/core/       12/18 (6 uncovered)
  src/utils/        5/12 (7 uncovered)
  src/api/           8/8 (fully covered)
  tests/             0/5 (5 uncovered)
  src/io/           3/8 (5 uncovered)

Top uncovered by downstream impact:
  SeqBuilder    [type]     -- 0 assertions, 4 dependents
  parse_config  [function] -- 0 assertions, 3 dependents
  Session       [type]     -- 0 assertions, 2 dependents

Full listing: cog index --verbose
Uncovered only: cog index --uncovered
```

**关键变化**：
- 默认输出是摘要而非全量列表（全量列表对 500+ entity 的模型无 actionable 价值）
- 按模块聚合 → agent 快速定位"哪个目录最需要 attention"
- 未覆盖 entity 按**下游依赖数**排序 → agent 优先关注影响面大的

**实现改动**：`src/command/index_cmd.rs` 默认模式切换为摘要；`src/format/text.rs` 新增摘要渲染。

---

## 5. 工作流状态机修订

### 5.1 当前问题

当前状态机（`workflow/state.rs`）：
```
Uninit → Ready { FreshScan/Exploring/Assessing/PostChange/Debugging } → Changing → Ready
```

三个结构性问题：

1. **`Changing` 追踪的信息 agent 已经知道。** agent 就是那个在改代码的实体——它不需要 cog 告诉它"你正在改代码"。`Changing` 状态存储的 `description` 和 `affected_entities` 对 `cog next` 的建议没有决策价值。

2. **`start-change`/`finish-change`/`abort-change` 是仪式化命令。** 它们在 SWE-CI 中零使用，因为 agent 的变更边界是模糊的——一个 epoch 可能包含多个独立修改，不存在清晰的"开始"和"结束"。

3. **`Assessing` 与 `Exploring` 在建议引擎中完全等价。** `suggest_for_ready(Assessing)` 的唯一建议是 "start an experiment"，这在 `Exploring` 中同样适用。两个 phase 的区分没有带来建议质量的提升。

### 5.2 修订方案：去掉 Changing，用 sync 自动检测变更边界

核心思路：不要求 agent 声明"我开始改代码了"。state machine 通过 `sync --update` 自动检测代码是否发生了变化——如果 sync 创建或删除了 entity，说明代码空间已经不同于模型，触发 PostChange。

```
WorkflowState:
  Uninit
  Ready {
    FreshScan    sync/init 刚执行，模型新鲜
    Exploring    默认状态：浏览、查询、记录断言
    PostChange   sync 检测到代码变更，模型需要同步更新
    Debugging    retract 触发 TMS 级联，需要解决
  }
```

**状态转换规则**：

```
Uninit
  --init()--> Ready/FreshScan

Ready/FreshScan
  --assert()/query()/depend()--> Ready/Exploring
  --sync(update, drift detected)--> Ready/PostChange

Ready/Exploring
  --sync(update, drift detected)--> Ready/PostChange
  --retract()--> Ready/Debugging
  --assert()/query()/impact()/depend()--> Ready/Exploring (stay)

Ready/PostChange
  --assert()--> Ready/Exploring           (agent recorded a correction)
  --retract()--> Ready/Debugging          (retract during post-change = trouble)

Ready/Debugging
  --verify(clean)--> Ready/Exploring      (issues resolved)
  --verify(fail)--> Ready/Debugging       (issues persist, stay)
```

**关键转换 `--sync(update, drift)--> PostChange`**：

这是整个状态机的核心转换，替代了旧的 `start-change → Changing → verify → PostChange` 路径。`sync --update` 执行增量扫描时，如果创建了新 entity 或删除了 stale entity，说明代码空间发生了变化。此时状态进入 PostChange，`cog next` 的建议从"explore and model"切换到"record corrections for changed entities"。

如果 `sync` 没有检测到任何 drift（代码未变），状态保持当前 phase 不变。agent 可以继续在 Exploring 中浏览和建模，无论它是否"打算"改代码。

**被移除的 phase**：

| Phase | 移除理由 |
|-------|---------|
| `Changing` | agent 自己知道在改代码；`sync` 自动检测变更替代了手动声明 |
| `Assessing` | 与 `Exploring` 在建议引擎中等价；impact/trace 后不改变建议内容 |

**被移除的命令**：

| 命令 | 移除理由 |
|-------|---------|
| `start-change` | 不再需要——变更检测是自动的 |
| `finish-change` | 不再需要——`assert` 触发 PostChange → Exploring 过渡 |
| `abort-change` | 不再需要——没有 Changing 状态可以放弃 |
---

## 6. 输出格式规范

### 6.1 下降 checkpoint 标记

每种输出类型应在其第一行表明它在下降协议中的位置：

| 输出类型 | checkpoint 信号 |
|---------|----------------|
| `cog next` | `State: Ready/Exploring` — 表明当前下降阶段 |
| `cog impact` | `WARNING: Risk reflects structural dependencies only` — 表明潜空间局限 |
| `cog experiment evaluate` | `Scout before implementing:` — Phase 2→3 转换信号 |
| `cog query` | assertions 列表 — 即时的知识状态 |
| `cog assert` | `SeqBuilder now has N assertions:` — 更新后的知识状态 |
| `cog sync` | `+3 unmodeled` — 代码空间→潜空间的 drift 信号 |

### 6.2 格式化约定

每条命令输出的第一行应以 `[COG:<command>]` 前缀标识输出类型，方便 LLM 做结构化解析：

| 元素 | 格式 | 示例 |
|------|------|------|
| 命令前缀 | `[COG:<command>]` | `[COG:impact]`, `[COG:query]`, `[COG:next]` |
| Entity | `name [kind]` | `SeqBuilder [type]` |
| Assertion ID | 8-char short ID | `a1b2c3d4` |
| Assertion 摘要 | `[kind] short_id: claim` | `[contract] a1b2c3d4: returns SeqBuilder...` |
| 状态标记 | `[status]` | `[uncertain]` |
| 新创建 | `[new]` | `[contract] a1b2c3d4: ... [new]` |
| Risk | `HIGH/MEDIUM/LOW (0.XX)` | `LOW (0.30)` |
| 建议 | `Next: command` | `Next: cog retract e5f6a7b8 --reason "superseded"` |
| 警告 | `WARNING: text` | `WARNING: Risk reflects structural dependencies only.` |
| 成功确认 | `OK: text` | `OK: SeqBuilder [type] -- read __call__ implementation` |
| 操作标签 | `[read]` / `[assert]` / `[verify]` | Scout 段落中引导 agent 的下一步操作 |
| Grounds | `source:qualifier` | `code:SeqBuilder.__init__`, `plan:refactor` |

### 6.3 Grounds 格式

`--grounds` 使用 `source:qualifier` 格式，中间以冒号分隔：

| 来源 | 格式 | 示例 |
|------|------|------|
| 代码引用 | `code:<entity_name>` | `code:SeqBuilder.__init__` |
| 设计意图 | `plan:<description>` | `plan:refactor auth module` |
| 实现发现 | `implementation_discovery:<context>` | `implementation_discovery:dbrattli epoch 3` |
| 自由注释 | `note:<text>` | `note:observed during review` |
| 元反馈 | `meta-loop:<context>` | `meta-loop:cog-self-modeling` |
不遵循推荐格式的 grounds 仍会被接受（回退为 `note:<text>`），但 agent 应尽可能使用上表中的标准 source 以便工具链处理。`Grounds::validate_format()` 仅校验 source 和 detail 非空，不强制白名单。

### 6.4 Token 优化规则

1. **UUID 短格式**：默认使用 8-char short ID，完整 UUID 仅 `--verbose` 显示
2. **Evidence 折叠**：`grounds: code:SeqBuilder` 一行，不展开
3. **列表上限**：默认最多 10 条，超出 `... and N more`
4. **空值省略**：某类为空时，不展示该类别标签
5. **label: value** 一行：不用多行嵌套缩进

### 6.5 TMS 语义保留

`retract` 将 assertion 状态改为 `Retracted`，但不删除 `assertion_relations` 表中的 `depends_on` 边——推理结构需要保留以便 TMS 级联正确传播 `Uncertain` 状态。级联逻辑读取的是关系，不是 assertion 的当前状态。

## 7. 完整 CLI 输入输出接口汇总

以下表格是 V2 设计的完整接口契约。每条命令的输入签名和输出格式均按本设计文档 §4 的定义。

### 7.1 命令总览

| # | 命令 | 频率 | 支柱 | 功能 |
|---|------|------|------|------|
| 1 | `sync` | 高频 | 入口 | 幂等全量扫描：创建 entity + relation，清理 stale |
| 2 | `next` | 高频 | 出口 | 统一入口：状态摘要 + 建议列表 |
| 3 | `query` | 高频 | 出口 | 实体认知卡片 |
| 4 | `impact` | 高频 | 出口 | 变更影响面 + 风险评估 + scout 指引 |
| 5 | `trace` | 低频 | 出口 | 依赖链追溯 |
| 6 | `assert` | 高频 | 入口 | 创建断言，返回 entity 即时上下文 |
| 7 | `retract` | 高频 | 入口 | 撤销断言，返回级联影响 + entity 即时上下文 |
| 8 | `depend` | 低频 | 入口 | 记录实体关系 |
| 9 | `index` | 低频 | 出口 | 实体覆盖摘要（默认摘要，--verbose 全文） |
| 10 | `experiment` | 中频 | 映射 | 下降协议核心通路：try/start/hypothesize/evaluate/commit/discard |
| 11 | `stats` | 低频 | 出口 | 详细统计（降级为 `next --verbose` 展开） |
| 12 | `export` | 低频 | 出口 | 导出模型（json/toml/dot） |
| 13 | `backup` | 低频 | 管理 | 全量备份 create/list/restore/drop |
| 14 | `delete-entity`| 低频 | 管理 | 删除实体及关联数据 |

### 7.2 高频命令输入输出契约

#### `cog next` — 统一入口

```
输入:
  cog next                    # 基础模式：状态 + 建议
  cog next --verbose          # 展开 stats 详细信息

默认输出:
  Line 1: State: <WorkflowState>        # Ready/FreshScan | Ready/Exploring | Ready/PostChange | Ready/Debugging
  Line 2..N: Experiment: <status> <short_id> — "<description>"   # 每个活跃实验一行；无实验时显示 "Experiment: none"
  Line N+1: Model: <N> entities, <M> assertions (<active> active, <retracted> retracted)
  Line N+2: Coverage: <P>% (<covered>/<total>)
  Line N+3: (blank)
  Lines N+4+: Suggestions:                 # 0-N 条，每条含 [kind] + 描述 + Next: <command>
  Last line: Status: OK

额外输出 (stagnation 触发时):
  WARNING: Model unchanged in recent operations. ...
    Next: Start with: cog experiment try <target> --kind correction --claim "..." --grounds "code:<target>" --desc "..."

额外输出 (stale experiment 触发时):
  WARNING: Experiment <short_id> ("<description>") has been evaluated but not committed/discarded in N operations. ...
    Next: cog experiment commit <id>  # or: cog experiment discard <id>

多实验示例:
  Experiment: evaluated a1b2c3d4 — "what if we change X?"
  Experiment: draft e5f6a7b8 — "refactor auth module"

#### `cog query` — 实体认知卡片

```
输入:
  cog query <entity>          # 默认: active assertions
  cog query <entity> --all    # 含 retracted assertions
  cog query <entity> --compact  # 单行模式

默认输出:
  Line 1: <entity_name> [<kind>]
  Line 2: (blank)
  Line 3: assertions (<N> active, <M> retracted):
  Lines 4+: [kind] <short_id>: <claim>
              grounds: <source>
  (blank)
  relations (<N>):
    -> <relation_kind> <target_entity> [<kind>]
    <- <relation_kind> <source_entity> [<kind>]

--compact 输出:
  <entity_name> [<kind>] -- <N> active:
    [kind] <short_id>: <claim>
    ...
```

#### `cog impact` — 影响面分析

```
输入:
  cog impact <entity>               # 默认: 风险概览 + downstream 摘要
  cog impact <entity> --verbose      # 展开 downstream 详情

默认输出:
  Line 1: Impact for: <entity_name> [<kind>]
  Line 2: (blank)
  Line 3: Risk: <HIGH|MEDIUM|LOW> (<0.XX>)
  Lines 4-6: Downstream: <N> entities (<covered> covered, <blind> blind)
             Active assertions at stake: <N>
             WARNING: Risk reflects structural dependencies only. ...
  Line 7: (blank)
  Lines 8+:  Downstream:
               <entity> [<kind>]    covered|blind (<N> assertions)
  (blank)
  Lines: Next: For details: cog impact <entity> --verbose
         Next: Test your plan safely: cog experiment try ...
```

#### `cog assert` — 创建断言

```
输入:
  cog assert <entity> --kind <kind> --claim "<text>" --grounds "<source>"
  cog assert <entity> --kind <kind> --claim "<text>" --grounds "<source>" --depends-on <id>

默认输出:
  Line 1: Created <short_id> [<kind>] on <entity_name>
  Line 2:   "<claim>"
  Line 3: (blank)
  Line 4: <entity_name> now has <N> active assertions:
  Lines 5+: <i>. [kind] <short_id>: <claim>    [new]   ← 仅新创建的标记
  (blank)
  (仅当同 kind 重复时):
  WARNING: <entity_name> already has <N> <kind> assertion(s). ...
    Next: To replace: cog retract <short_id> --reason "..."

副作用:
  WorkflowState: (FreshScan|PostChange) → Exploring
```

#### `cog retract` — 撤销断言

```
输入:
  cog retract <id> --reason "<why>"

默认输出:
  Line 1: Retracted <short_id> [<kind>] on <entity_name>
  Line 2:   Reason: "<reason>"
  Line 3: (blank)
  Lines 4+ (仅当有级联时):
           Cascade: <N> assertion(s) affected
             <short_id> [<kind>] -> uncertain (ground weakened)
               "<claim>"
  Line:    (blank)
  Line:    <entity_name> now has <N> active assertions:
  Lines:     [kind] <short_id>: <claim> [uncertain]   ← 状态变化标记
  (blank)
  (仅当有 uncertain 时):
  Next: <short_id> is now uncertain. Re-verify it:
      cog query <entity_name> --all

副作用:
  WorkflowState: → Debugging
```

### 7.3 中频命令输入输出契约

#### `cog sync` — 模型与代码幂等同步

> **设计偏差**：§4.2 和本节原设计了一个两步 sync 流程（`sync` 仅检测 drift → `sync --update` 应用变更）。
> 实际实现采用了更简洁的幂等模型：`sync` 默认执行完整扫描并写入，`--dry-run` 仅报告。
> 理由见偏差说明"§4.2 sync 命令设计——init 合并"。

```
输入:
  cog sync .                      # 幂等全量同步（默认）
  cog sync . --dry-run             # 仅报告变更，不写入
  cog sync src/                    # 限定扫描路径
  cog sync . --lang python,rust    # 限定语言
  cog sync . --depth 2             # 限定目录深度

默认输出 (有变更):
  Sync: <N> files scanned (<lang_summary>)
    +<X> entities created, -<Y> removed, <Z> relations
    <kind>: <count>
  After sync: <total> entities, <A> assertions
  Next: cog index | cog impact <entity>

默认输出 (无变更):
  Sync: <N> files scanned (<lang_summary>)
  After sync: <total> entities, <A> assertions
  Model is up to date — no drift.
  Next: cog index | cog impact <entity>

--dry-run 输出:
  DRY RUN — no changes written
  Scanned <N> files (<lang_summary>)
  Would sync entities and relations.
  Next: Apply changes: cog sync .

副作用 (非 dry-run, drift detected):
  WorkflowState: Uninit → Ready/FreshScan; else → Ready/PostChange
```

#### `cog experiment` — 沙盘推演

```
子命令:
  cog experiment try <entity> --kind <k> --claim "<t>" --grounds "<s>" [--desc "<d>"] [--depends-on <id>]
  cog experiment start <entity> --desc "<d>"
  cog experiment hypothesize <id> --assert|--delete|--relation ...
  cog experiment evaluate <id>
  cog experiment commit <id>
  cog experiment discard <id>
  cog experiment list
  cog experiment report <id>

try 输出 (高风险):
  Experiment <short_id>: "<description>"
  Hypothesis:
    + [<kind>] <entity>: "<claim>"
  Evaluation:
    Risk: <HIGH|MEDIUM|LOW> (<0.XX>)
    Contradictions: <N>
      <i>. <description> (<source>)
    Affected assertions: <N>
    Cascade: <N> assertions -> uncertain
  Scout before implementing:
    <entity> [<kind>] -- <scout reason>
  Next: Discard: cog experiment discard <id>
  Next: Adjust hypothesis: cog experiment hypothesize <id> --kind correction --claim "..." --grounds "code:<entity>"
  Next: Proceed to implementation: cog experiment commit <id>

try 输出 (低风险):
  Experiment <short_id>: "<description>"
  Evaluation:
    Risk: LOW (<0.XX>)
    Contradictions: 0
    Affected assertions: <N>
    Cascade: 0
  Scout before implementing:
    <entity> [<kind>] -- verify current signature
  Next: Safe to proceed: cog experiment commit <id>
```

### 7.4 低频命令输入输出契约

#### `cog index` — 实体覆盖摘要

```
输入:
  cog index                     # 默认: 覆盖摘要
  cog index --uncovered         # 仅无 assertion 的 entity
  cog index --top <N>           # assertion 最多的 top-N
  cog index --prefix <p>        # 按模块前缀过滤
  cog index --kind <k>          # 按 entity kind 过滤
  cog index --verbose           # 全文列表 (旧行为)

默认输出:
  Coverage: <covered>/<total> (<P>%)
  By module (top <N> uncovered):
    <module_path>/    <covered>/<total> (<uncovered> uncovered)
  Top uncovered by downstream impact:
    <entity> [<kind>] -- <N> assertions, <M> dependents
  Full listing: cog index --verbose
  Uncovered only: cog index --uncovered
```

#### `cog trace`, `cog depend`, `cog sync`, `cog export`, `cog backup`, `cog stats`, `cog delete-entity`

不修改。保持现有输入输出格式。

### 7.5 全局选项

```
cog [--db <path>] [--output text|json] <command> [args]

--db <path>        数据库路径 (默认: .cog/cog.db; 环境变量: COG_DB)
--output <format>  输出格式: text (默认) | json
```

### 7.6 WorkflowState 影响总结

| 命令 | 进入的状态 |
|------|-----------|
| `sync` (首次，Uninit → FreshScan) | Ready/FreshScan |
| `sync` (有 drift，非 dry-run) | Ready/PostChange |
| `sync` (无 drift) | 不变 |
| `sync --dry-run` | 不变（仅报告，不修改模型） |
| `assert` | (FreshScan|PostChange) → Exploring; else stay |
| `retract` | Ready/Debugging |
| `verify` (clean, from Debugging) | Ready/Exploring |
| `query`, `impact`, `depend`, `index`, `trace`, `experiment`, `export`, `backup`, `stats`, `delete-entity` | 不变 |

`cog next` 同时读取 `WorkflowState` 和所有活跃 experiment（通过扫描 `.cog/experiments/` 目录），两个维度的组合决定建议内容。支持多个并行活跃 experiment。详见 [LATENT_TO_CODE_MAPPING.md §7.3](./LATENT_TO_CODE_MAPPING.md#73-cog-next-合成逻辑)。

| experiment 命令 | 磁盘状态变化 |
|------|------|
| `start` / `try` | 写入 .json，status=Open/Evaluated |
| `evaluate` | status → Evaluated |
| `commit` | status → Committed（不再出现在活跃列表） |
| `discard` | status → Discarded（不再出现在活跃列表） |

> **设计偏差**：`cog next` 原设计使用单一 `ExperimentStatus` 字符串（如 "draft a1b2c3d4"），与 `WorkflowState` 做笛卡尔积匹配。
> 实际实现改为 `Vec<ActiveExperiment>`：实验状态不再与 workflow phase 做排列组合，而是分离为两阶段决策——
> 先处理实验优先级（draft > evaluated > phase 建议），再追加阶段特定建议，最后检测停滞。
> 理由：多并行实验使笛卡尔积爆炸（~15+ 组合），分离后更易维护且自动支持并行实验。

---

## 8. 实现路线图

### Phase 1：高频接口优化（入口 + 出口支柱）

对 agent 每 epoch 的认知循环影响最大。预期效果：减少 40% token 开销 + 消除统计分析瘫痪。

| 改动 | 文件 | 效果 |
|------|------|------|
| `next` 合并 stats + 停滞检测 + 覆盖率自适应 | `workflow/suggestions.rs` | 每 epoch -1 CLI 调用 |
| `next` 输出精简 | `format/text.rs` | 每 epoch -200 tokens |
| `assert` 返回 entity 当前状态 | `command/assert_cmd.rs` | 每 assert -1 query 调用 |
| `assert` 同 kind 提示 | `command/assert_cmd.rs` | 减少噪声 assertion |
| `index` 默认摘要 | `command/index_cmd.rs` | 输出从 ~650 行 → ~20 行 |
| `query` `--compact` 模式 | `command/query.rs`, `format/text.rs` | 嵌入式使用场景 |

### Phase 2：新增 sync 命令 + impact 增强（支柱 1 + 出口）

| 改动 | 文件 | 效果 |
|------|------|------|
| 新增 `sync` 命令 | `command/sync_cmd.rs`（新文件） + 从 `verify.rs` 提取 `detect_drift()` 共用 | 替代 verify --scan + init 混乱；`--update` 路径复用 `init_cmd.rs` 的 entity + relation 创建 |
| impact RiskAssessment 增强（coverage, caveats） | `space/risk.rs`, `space/semantic.rs`, `format/text.rs` | 消除虚假安全感 |

### Phase 3：experiment 增强 + retract 增强（映射支柱）

| 改动 | 文件 | 效果 |
|------|------|------|
| `experiment try` 一行命令 | `cli/experiment.rs`, `command/experiment_cmd.rs` | experiment 使用率提升 |
| `experiment evaluate` 输出 scout 指引 | `experiment/report.rs`, `format/text.rs` | Phase 2->3 桥梁 |
| `retract` 返回 entity 状态 + 级联 | `command/retract.rs`, `format/text.rs` | retract 后无额外 query |
| experiment 允许 re-evaluate | `experiment/session.rs` | 支持 Phase 3/4 循环 |

### Phase 4：状态机精简 + 清理

| 改动 | 文件 | 效果 |
|------|------|------|
| 移除 `start-change`/`finish-change`/`abort-change` 命令 | `cli/mod.rs`, `cli/args.rs` | 接口从 18 -> 15 个命令 |
| 移除 `WorkflowPhase::Changing` 和 `Assessing` | `workflow/state.rs` | 状态机从 7 态 -> 5 态 |
| `sync --update` 触发 PostChange 转换 | `command/sync_cmd.rs`, `workflow/state.rs` | 变更检测自动化 |
| `stats` 降级为 `next --verbose` 展开 | `cli/mod.rs` | 减少接口表面积 |

---

## 9. 版本演进

本文档是 `CLI_INTERFACE_REDESIGN.md` 的后续版本。第一版（2026-06-08）基于 SWE-CI 10-task pilot 轨迹分析，提出了入口和出口的具体命令设计——包括 `next` 合并 stats、`sync` 命令、`assert` 即时反馈、`impact` 风险增强、`index` 默认摘要、`experiment try`。这些设计经审阅后全部保留，构成了本文档 §4 的核心。

V2 在第一版基础上完成了三件事：
1. 引入**设计哲学框架**（四条原则、三大支柱），将分散的命令设计统一为 cohesive model
2. 引入**下降协议**和映射支柱，补上了从潜空间推理到代码空间实现之间的 gap
3. 重设计**工作流状态机**，去掉 `Changing` 状态和仪式化命令，用 `sync --update` 自动检测变更边界

---

## 附录 A：SWE-CI 根因到接口改动的映射

| SWE-CI 根因 | 接口改动 | 预期效果 |
|------------|---------|---------|
| 启动仪式税（3 命令开幕） | `next` 合并 stats + `sync` 替代 verify | 每 epoch -2 CLI 调用 |
| 虚假安全感（impact "low risk"） | impact caveats + blind 标记 + scout 指引 | agent 不再对 0-downstream entity 盲目信任 |
| 过度建模（42.6% cog 开销） | assert 同 kind 提示 + next 覆盖率自适应 | 覆盖充分时引导停止建模 |
| 分析瘫痪（epoch 11 零 edits） | next 停滞检测 | 停滞时引导实现而非继续分析 |
| TMS 未激活（0 次 depends_on） | assert 输出中显示已有 assertions + retract 展示级联 | agent 看到 assertion 间关系后自然使用 depends_on |
| experiment 闲置 | experiment try 一行命令 + scout 指引 | 零摩擦启动沙盘推演 |
| 失败不回流 | retract 后 context + fragility 引导 | agent 把实现失败记录下来 |
| 输出冗长（index 650 行） | index 默认摘要 | 输出缩减 30x |
