# 从潜空间到代码空间：认知模型的映射问题

> 日期：2026-06-08
> 前置阅读：`docs/COGNITIVE_MODEL_DESIGN.md`, `docs/RUST_ARCHITECTURE_REDESIGN.md`
> 基于：SWE-CI benchmark 10-task pilot 轨迹分析

---

## 1. 问题定义

### 1.1 什么是"映射问题"

Cog 构建了一个**潜空间**（latent space）——对代码的压缩、抽象表征：

```
代码空间                          潜空间
┌──────────────────┐             ┌──────────────────────┐
│ src/auth.py:42   │   压缩      │ Entity("auth::login")│
│ def login(u, p): │  ────────→  │ Assertion("返回 None │
│   ...47 lines... │   建模      │   意味着认证失败")   │
│   return token   │             │ Impact: 4 downstream │
└──────────────────┘             └──────────────────────┘
```

压缩本身是 cog 的核心价值——正如设计文档所述，"潜空间将规划变成图搜索"。**但 agent 最终必须在代码空间中执行变更**。从潜空间的抽象推理回到代码空间的具象实现，这个"下降"过程存在根本性的 fidelity gap。

### 1.2 两次信息压缩

潜空间的构建经过了两次有损压缩：

| 压缩 | 输入 | 输出 | 丢失的信息 |
|------|------|------|-----------|
| **结构压缩** | 源码文本（AST） | entity name + kind + contains/calls/uses 关系 | 控制流、变量生命周期、异常处理、concurrency 语义 |
| **语义压缩** | agent 对代码的理解 | assertion 文本（contract/intent/invariance/fragility/correction） | 隐含假设、边界条件、上下文依赖、性能特征 |

两次压缩后，潜空间中"0 downstream, low risk"的 entity（如 dbrattli 中的 `SeqBuilder`）实际上可能承载着复杂的运行时协议（Python generator 的 `yield-from` 语义）。这个 gap 在推理阶段是隐藏的，在实现阶段才暴露。

### 1.3 为什么这是根本性问题

这不是 prompt 设计问题，不是 agent 能力问题，而是**信息论约束**。任何抽象都会丢失信息，丢失的信息在还原时必然成为未知。关键不在于消除 gap——这不可能——而在于**设计一个能在 gap 暴露时安全降落的机制**。

当前 cog 的失败模式正是缺少这个机制：agent 在潜空间中完成推理后直接跳入代码实现，gap 暴露时没有 checkpoint 可以回退，没有将发现回流到模型的通路。

---

## 2. SWE-CI 证据

### 2.1 dbrattli：虚假安全感的代价

| 维度 | 数据 |
|------|------|
| Entity | `SeqBuilder` (type) |
| cog impact 结论 | "Low risk (0.10), 0 downstream, 2 assertions" |
| 实际风险 | Python generator protocol (`yield-from` 语义) |
| 结果 | 8 个 epoch 在 4 种实现间振荡，42.6% 的工具调用是 cog 命令 |

**发生了什么**：`cog impact` 返回 `downstream: 0`，agent 理解为"可以放心改"。但实际上 `SeqBuilder.__call__` 涉及 Python generator protocol（`__next__`、`send()`、`yield-from`、`StopIteration`），这些都是运行时行为，不在结构依赖图中。潜空间的推理给出了错误的安全信号。

### 2.2 argoproj：推理正确但实现炸裂

| 维度 | 数据 |
|------|------|
| 初始 gap | 64 |
| Epoch 2 行为 | 5 次 `cog impact` 后大范围修改 3 个文件 |
| 结果 | gap 从 55 飙升至 70（+15 个新失败） |
| 最终 | 15 epochs 解决（baseline 20 epochs 未解决） |

**发生了什么**：agent 的推理在潜空间是正确的——修改 3 个文件的计划本身没问题。但实现时对 `pydantic` 的 validation 和 `argo-workflows` SDK 的隐式约束（必须同时修改 model definition 和 YAML template）缺乏了解。这些约束既不在 entity graph 也不在 assertion 中。agent 执行了计划的前半部分（改 model），没执行后半部分（改 template），因为不知道后半部分存在。

### 2.3 9001__co：分析瘫痪——潜空间中打转

| 维度 | 数据 |
|------|------|
| Epoch 11 | 128K tokens，140 工具调用，0 edits |
| cog 命令 | 15 次（query x4, impact x3, next x2, assert x3, verify, stats, export） |
| epoch 6-13 | gap=1 卡了 8 个 epoch，114 次 cog 命令 |

**发生了什么**：agent 反复在潜空间中探索（query→impact→assert→retract），但每次试错都只是在认知模型中调整，没有真正去代码空间做最小变更。潜空间变成了一个舒适的沙盒——可以无限探索，没有试错代价。

### 2.4 amaranth（正面案例）：当推理与实现一致时

| 维度 | 数据 |
|------|------|
| 核心决策 | 将 Signal 类从 `__init__` 改为 `__new__` + `__init__` 分割 |
| cog impact | 显示 4 downstream，risk 0.30 |
| 关键行为 | 97 次 bash smoke test（baseline 仅 17 次） |
| 结果 | 4 epochs 解决，零回归 |

**发生了什么**：`__new__/__init__` 分割是一个**静态结构变更**——正好在 cog 的扫描能力范围内。潜空间推理（"4 downstream, low risk"）与代码空间实现之间的 gap 很小。而且 agent 做了 97 次增量测试来验证每次小改动的正确性——用高频反馈弥补了 gap。

---

## 3. 三种失败模式

### 3.1 抽象坍塌（Abstraction Collapse）

> 潜空间的推理正确，但代码空间的实现细节不在模型中。

**特征**：`cog impact` 返回低风险，agent 信心充足，实现时撞上未建模的约束。

**示例**：dbrattli 中 `SeqBuilder.__call__` 的 generator protocol 交互。模型中有 entity（`SeqBuilder`）、有 assertion（"returns SeqBuilder for chaining"），但没有 generator protocol 的运行时语义。agent 基于模型做了正确推理（"0 dependents, safe to change"），但代码实现需要处理 `send()`/`next()`/`yield-from`/`StopIteration` 这个完整的运行时协议。

**根因**：模型压缩了控制流和运行时行为，而这些恰好是修改实现时最需要的信息。

### 3.2 级联发现（Cascading Discovery）

> 实现过程中发现新的约束，这些约束颠覆了推理前提。

**特征**：初始变更触发了预期外的级联效应，需要重做大量推理。

**示例**：argoproj 中修改 3 个文件时，agent 不知道必须同时修改 YAML template 文件——这个依赖是跨文件、隐式的、不在 `import` 语句中。当 agent 修改了 model definition 后，pytest 报告了 template validation 失败，此时才意识到需要追加修改。但原有的 experiment 计划已经包含了 3 个文件的操作，追加的新约束需要重新评估。

**根因**：模型的依赖图只覆盖了语言级的关系（import、调用），没有覆盖框架级、配置级的隐式依赖。

### 3.3 沉没成本陷阱（Sunk Cost Trap）

> Agent 在潜空间投入了大量推理，形成的断言成为"必须执行"的负担。

**特征**：模型中有大量关于"计划做什么"的断言，但这些断言在实现失败后没有被清理，成为下一次推理的噪音。

**示例**：dbrattli 中 agent 对 `SeqBuilder.__call__` 做了 12 次 retract——每次尝试一个实现方案，先 assert 它是对的认识，实现后发现不对，再 retract。模型变成了 agent 错误的历史记录。更严重的是，这些已 retract 的断言不会自动清理——它们留在模型中作为噪音，在下一次推理时会被错误引用。

**根因**：模型的写操作立即可见，但没有"撤销推理轨迹"的机制。retract 标记了一条 assertion 为废弃，但不撤销 assertion 间的 `depends_on` 链——这些链仍然存在于 assertion_relations 表中。

---

## 4. 下降协议（Descent Protocol）

### 4.1 概念

下降协议是一组**操作序列约定**，指导 agent 如何安全地从潜空间推理下降到代码空间实现。它不是新的 CLI 命令，而是在现有命令之上叠加的使用模式。

核心思想与飞行器下降类似：不能从巡航高度（抽象计划）直接降落（具象实现），必须通过多个 checkpoint 逐级下降，每个 checkpoint 验证当前高度与地面的对齐。

```
潜空间（推理层）                      代码空间（实现层）
══════════════════                  ══════════════════
                                    
Phase 1: SURVEY                     不会修改任何代码
  理解影响面、风险、已有合同
  工具: impact, trace, query
  
         ↓ checkpoint: 推理是否自洽？
         
Phase 2: HYPOTHESIZE                不会修改任何代码
  沙盘推演变更计划
  工具: experiment try
  
         ↓ checkpoint: 推演有无矛盾？
         
Phase 3: SCOUT                      开始读代码
  验证潜空间假设是否与现实一致
  工具: 源码阅读 + 与模型比对
  
         ↓ checkpoint: 假设是否成立？
                                       
Phase 4: PROBE                      最小代码变更
  做第一个实体的小改动，运行测试
  工具: 编辑 + pytest + 查看反馈
  
         ↓ checkpoint: 最小变更是否通过？
         
Phase 5: COMPLETE                   完整代码变更
  实现剩余改动，更新模型
  工具: 编辑 + cog assert/retract + cog verify
```

### 4.2 每层的失败处理

| Phase | 失败类型 | 处理策略 | 模型操作 |
|-------|---------|---------|---------|
| SURVEY | 推理不自洽（影响面矛盾） | 重新 survey | 无（尚未写入模型） |
| HYPOTHESIZE | 推演有矛盾 | 修改假设或放弃 | experiment discard |
| SCOUT | 假设与现实不符 | 更新假设，重新推演 | experiment hypothesize（追加） |
| PROBE | 最小变更失败 | 记录失败原因，回溯到上一步 | assert fragility + experiment discard |
| COMPLETE | 整体变更失败 | 回到最后一个成功的 checkpoint | retract 错误断言 + assert correction |

关键机制是：**SCOUT 和 PROBE 的失败不是浪费，而是被结构性地记录为模型的更新**。当 agent 在 dbrattli 中发现 `SeqBuilder.__call__` 涉及 generator protocol 时，它应该：

```sh
# 不丢弃 experiment，而是：
cog assert SeqBuilder --kind fragility \
    --claim "修改 __call__ 必须考虑 yield-from protocol 交互——静态依赖分析不可见" \
    --grounds "implementation_discovery:dbrattli epoch 3"
cog experiment hypothesize <id> --delete SeqBuilder  # 从计划中移除该 entity
cog experiment evaluate <id>                          # 重新评估
```

这样，失败变成了模型中有价值的知识——下次推理时 `cog query SeqBuilder` 会返回这条 fragility，提醒它小心。

### 4.3 与现有 experiment 系统的关系

当前 experiment 系统设计用于 "what if" 推演（`start → hypothesize → evaluate → commit/discard`），完全在潜空间内运作。下降协议的 Phase 3-5 是对 experiment 系统在代码空间侧的补充：

| experiment 命令 | 下降协议中的角色 |
|----------------|----------------|
| `start` | Phase 2 起点：加载依赖子图 |
| `hypothesize` | Phase 2 核心：注入计划中的变更 |
| `evaluate` | Phase 2 终点 + Phase 3/4 的回归入口（实现中发现新信息后，重新 evaluate） |
| `commit` | Phase 5 起点：确认推演有效，开始将操作回放到真实模型 |
| `discard` | 任何 phase 失败时：丢弃推演，但保留实施过程中发现的 fragility/correction 断言 |

不需要新的 experiment 子命令。需要的是：
1. `evaluate` 输出中增加 **scout 建议**——列出需要在实际代码中验证的 entity
2. 允许在 experiment 生命周期内**多次 hypothesize + evaluate**（当前已支持）
3. 允许 agent 在实现过程中（Phase 4/5）不通过 experiment 直接 model（assert/retract），但关联到 experiment context

---

## 5. CLI 设计影响

下降协议对 CLI 设计提出了三个核心要求：

### 5.1 evaluate 输出必须包含 scout 指引

当前的 `experiment evaluate` 输出 contradictions 和 risk score，但没有告诉 agent "你应该去读哪些实际代码来验证假设"。缺少 scout 阶段的指引，agent 要么直接跳到实现（跳过 SCOUT），要么在 SCOUT 阶段漫无目的地读代码。

**要求**：`evaluate` 输出中增加一个 `Scout suggestions` 段落，列出：
- 与 planned change 有直接关系的 entity（需要验证它们的现有断言是否还成立）
- 无 assertion 但被推演影响到的 entity（盲区，需要优先检查）
- 边界 entity（subgraph boundary 上的 entity，包含因 BFS 子图边界限制而未加载全部依赖数据的 entity）

### 5.2 写操作必须即时返回状态（支持 Probe 循环）

在 Phase 4 PROBE 阶段，agent 做最小变更后需要快速记录发现、更新模型、决定下一步。如果每次 assert/retract 之后都需要额外的 query 来"看看现在是什么状态"，probe 循环的摩擦就太高了。

**要求**：`assert` 和 `retract` 的输出必须包含受影响 entity 的当前完整状态。这是 `CLI_INTERFACE_V2.md` 中的"即时上下文"原则（原则 2）——在下降协议中，这个原则不仅关乎效率，还关乎 feedback loop 的紧密程度。

### 5.3 experiment 生命周期需要贯穿下降全过程

当前的 experiment 设计假设：`start → hypothesize → evaluate → commit/discard` 是一个完整的、封闭的流程。但下降协议要求 agent 在 SCOUT 阶段发现新信息后重新 hypothesize，在 PROBE 失败后追加 fragility 断言。因此 experiment 的生命周期必须是迭代式的：

**要求**：
- `hypothesize` 在 `Open` 和 `Evaluated` 状态下均可调用（修改 `Experiment::hypothesize` 的 status 检查，移除仅限 `Open` 的约束）
- `evaluate` 在 `Open` 和 `Evaluated` 状态下均可调用（`mark_evaluated` 改为幂等：若已在 `Evaluated` 状态则跳过）

---

## 6. 降级决策：简化方案

下降协议的完整五阶段流程是理想形式。在实际 SWE-CI 场景中，许多 entity 的变更不涉及复杂推理。一个合理的降级决策表：

| 场景 | 下降协议使用程度 | 理由 |
|------|----------------|------|
| 修改函数签名 | Phase 1-5 完整流程 | 结构变更，影响面大 |
| 修改内部实现（不改接口） | Phase 1-3 + 直接 Phase 5 | 跳过 HYPOTHESIZE 和 PROBE——结构影响需要评估，但不需要沙盘推演 |
| 修改 1-2 行逻辑 | Phase 1 + 直接 Phase 5 | 跳过大部分 checkpoint |
| 修复显而易见的 bug | 直接 Phase 5 | 跳过所有推理 |
| 添加新文件/模块 | Phase 1-2 + 直接 Phase 5 | 新 entity 没有下游，但需要确认不会与现有代码冲突 |

这个降级决策表应该反映在 `cog next` 的建议中——`next` 应能根据 entity 的特征（downstream 数量、assertion 覆盖度、kind）推荐适当的下降深度。

---

## 7. 与工作流状态机的关系：为什么是两个，不是一个

### 7.1 问题：单状态机方案不可行

下降协议引入 experiment 状态后，最直觉的做法是把 experiment 状态直接加入 `WorkflowState` 枚举：

```
Uninit
Ready { FreshScan | Exploring | PostChange | Debugging }
Experimenting             ← 新增
ExperimentEvaluated       ← 新增
```

这个方案在第一次具体场景推演时就崩溃了。考虑以下操作序列：

```
1. cog experiment start SeqBuilder     → agent 进入"Experimenting"
2. cog experiment evaluate a1b2c3d4   → agent 进入"ExperimentEvaluated"
3. 读代码验证假设（此时 workflow state 是 Ready/Exploring）
4. agent 改代码（sync --update 检测到 code drift → PostChange）
```

第 3-4 步时 agent 应该处于什么状态？它同时在 "ExperimentEvaluated"（潜空间中有一个已完成推演的 experiment）和 "Ready/Exploring" 或 "Ready/PostChange"（代码空间中正在验证或已变更）。这两个状态不是互斥的——一个在潜空间，一个在代码空间。它们可以——而且应该——同时存在。

把两者拍平为互斥的扁平枚举会产生不必要的组合爆炸：

```
有效的复合状态（部分列举）:
  Draft + FreshScan      / Draft + Exploring     / Draft + PostChange
  Evaluated + FreshScan  / Evaluated + Exploring / Evaluated + PostChange
  Evaluated + Debugging
  None + FreshScan       / None + Exploring      / None + PostChange    / None + Debugging
```

约 15 个有效复合状态。维护一个 15 态的扁平枚举等价于维护两个独立变量——但前者混淆了正交维度，后者保持了维度的独立性。

### 7.2 方案：两个独立状态变量

一个状态变量追踪代码空间活动，另一个追踪潜空间活动。它们在操作上是独立的——进入/退出 experiment 不改变 workflow state，同步代码变更也不改变 experiment status。

```
WorkflowState（追踪代码空间）:
  Uninit
  Ready { FreshScan | Exploring | PostChange | Debugging }

ExperimentStatus（追踪潜空间）:
  None          没有活跃 experiment
  Draft         experiment 已 start，正在 hypothesize
  Evaluated     experiment 已 evaluate，等待 commit 或 discard
```

代码空间变更不映射为独立状态——`sync --update` 检测到代码 drift 后自动进入 `PostChange`，agent 不需要手动声明 "开始改代码"（见 CLI_V2 §5.2）。`Scouting` 和 `Probing` 由 `ExperimentStatus::Evaluated` 组合任意 `WorkflowState` 来表达。`cog next` 的停滞检测机制会感知到 agent 长时间停留在特定组合中（如 Evaluated+Exploring 超过 N 个 epoch 无进展），触发相应建议。

### 7.3 `cog next` 合成逻辑

`cog next` 读取 `.cog/workflow_state.json` 获取 `WorkflowState`，扫描 `.cog/experiments/` 目录获取所有活跃 experiment（`Vec<ActiveExperiment>`）。两个独立输入通过两阶段决策合成建议：

**阶段 1：实验优先级**（phase-independent）

| 实验状态 | 建议优先级 |
|---------|-----------|
| 有 Draft 实验 | "N draft experiment(s) pending evaluation. Finish before starting new work." |
| 有 Evaluated 实验 | "N evaluated experiment(s) ready. Commit to proceed or discard." |

**阶段 2：阶段特定建议**（按 WorkflowPhase）

| WorkflowState | 建议 |
|---|---|
| Ready/FreshScan | "Start recording assertions for core entities." + orphan count |
| Ready/Exploring | 标准建议（assess, assert/impact per coverage, implement） |
| Ready/PostChange | "Code changed. Record corrections for changed entities." |
| Ready/Debugging | uncertain review + trace + verify |

**阶段 3：停滞检测**（附加）

| 规则 | 触发条件 | 建议 |
|------|---------|------|
| Verify 循环 | 最近 5 条 changelog 全为 Verify | "Consider implementing rather than further analysis." |
| Stale 实验 | Evaluated 实验后累积 >= 5 条 changelog | "Experiment <id> has been evaluated but not committed/discarded in N operations." |
| Guard | 最近 5 条 changelog 全为 Assert | 不触发（密集建模是进展信号） |

这种两阶段合成是确定性的——没有 ML 推断，没有启发式权重。`cog next` 是一个规则引擎，实验状态和 workflow phase 独立贡献建议，而非笛卡尔积。

## 8. 总结

| 要点 | 说明 |
|------|------|
| **映射问题是根本性的** | 任何抽象→实现的转换都有 fidelity gap。不是 bug，是信息论约束 |
| **当前 CLI 缺少下降机制** | experiment 系统只做潜空间推理，没有与代码实现形成闭环 |
| **下降协议不需要新命令** | 在现有 experiment 系统上增加 scout 指引 + probe 循环支持 |
| **失败必须有回流** | 实现失败 → 记录 fragility/correction → 模型学到教训 |
| **降级决策是必要的** | 不是所有变更都需要 5 阶段。简单变更应该跳过大部分 checkpoint |

这个文档定义了下降协议的概念框架。具体的 CLI 命令改动（`experiment evaluate` 输出增强、`assert`/`retract` 即时反馈、`next` 的降级建议）在 `CLI_INTERFACE_V2.md` 中展开。
