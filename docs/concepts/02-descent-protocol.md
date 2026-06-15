# 下降协议：从潜空间到代码空间

> 下降协议是一组操作序列约定，指导 agent 如何安全地从潜空间推理下降到代码空间实现。它不是新的 CLI 命令，而是在现有命令之上叠加的使用模式，应对 [01-failure-modes.md](01-failure-modes.md) 描述的 fidelity gap。

## 1 概念

核心思想与飞行器下降类似：不能从巡航高度（抽象计划）直接降落（具象实现），必须通过多个 checkpoint 逐级下降，每个 checkpoint 验证当前高度与地面的对齐。

```
潜空间（推理层）                      代码空间（实现层）

Phase 1: SURVEY                     不修改任何代码
  理解影响面、风险、已有契约
  工具: cog impact, cog trace, cog query

         ↓ checkpoint: 推理是否自洽？

Phase 2: HYPOTHESIZE                不修改任何代码
  沙盘推演变更计划
  工具: cog experiment try

         ↓ checkpoint: 推演有无矛盾？

Phase 3: SCOUT                      开始读代码
  验证潜空间假设是否与现实一致
  工具: 源码阅读 + 与模型比对

         ↓ checkpoint: 假设是否成立？

Phase 4: PROBE                      最小代码变更
  做第一个实体的小改动，运行测试
  工具: 编辑 + 测试 + cog assert/retract 记录发现

         ↓ checkpoint: 最小变更是否通过？

Phase 5: COMPLETE                   完整代码变更
  实现剩余改动，更新模型
  工具: 编辑 + cog assert/retract + cog sync + cog verify
```

## 2 每层的失败处理

| Phase | 失败类型 | 处理 | 模型操作 |
|-------|---------|------|---------|
| SURVEY | 推理不自洽 | 重新 survey | 无（尚未写入模型） |
| HYPOTHESIZE | 推演有矛盾 | 修改假设或放弃 | `experiment discard` |
| SCOUT | 假设与现实不符 | 更新假设，重新推演 | `experiment hypothesize`（追加） |
| PROBE | 最小变更失败 | 记录失败原因，回溯 | `assert fragility` + `experiment discard` |
| COMPLETE | 整体变更失败 | 回到最后成功的 checkpoint | `retract` 错误断言 + `assert correction` |

**关键机制**：SCOUT 和 PROBE 的失败不是浪费，而是被结构性地记录为模型更新。当 agent 在实现中发现 `SeqBuilder.__call__` 涉及 generator protocol 时，应该：

```sh
# 不丢弃 experiment，而是把发现记录为 fragility
cog assert SeqBuilder --kind fragility \
    --claim "修改 __call__ 必须考虑 yield-from protocol 交互——静态依赖分析不可见" \
    --grounds "implementation_discovery:dbrattli epoch 3"
```

这样失败变成模型中有价值的知识——下次推理时 `cog query SeqBuilder` 会返回这条 fragility，提醒小心。

## 3 与 experiment 系统的关系

experiment 系统设计用于"what if"推演（`start → hypothesize → evaluate → commit/discard`），完全在潜空间内运作。下降协议的 Phase 2 用它做沙盘推演，Phase 3-5 是它在代码空间侧的补充。

| experiment 命令 | 下降协议中的角色 |
|----------------|----------------|
| `start` | Phase 2 起点：加载依赖子图 |
| `hypothesize` | Phase 2 核心：注入计划中的变更（Open/Evaluated 状态均可调用） |
| `evaluate` | Phase 2 终点 + Phase 3/4 的回归入口（实现中发现新信息后重新 evaluate） |
| `commit` | Phase 5 起点：将推演的操作回放到真实模型 |
| `discard` | 任何 phase 失败时：丢弃推演，但**保留**实施过程中发现的 fragility/correction |

experiment 生命周期的两个关键性质支撑了下降协议：

1. **`hypothesize` 和 `evaluate` 是幂等可迭代的**——agent 在 SCOUT 发现新信息后可重新 `hypothesize` + `evaluate`，不必从头开始。
2. **`commit` 是确定性 replay**——不涉及 diff/merge，staged 操作按序回放到真实 DB，无 UUID 冲突。见 [architecture/07-experiment-layer.md](../architecture/07-experiment-layer.md)。

## 4 降级决策：不是所有变更都需要五阶段

完整五阶段是理想形式。许多 entity 的变更不涉及复杂推理。合理的降级表：

| 场景 | 使用程度 | 理由 |
|------|---------|------|
| 修改函数签名 | Phase 1-5 完整流程 | 结构变更，影响面大 |
| 修改内部实现（不改接口） | Phase 1-3 + 直接 Phase 5 | 跳过沙盘推演和 PROBE |
| 修改 1-2 行逻辑 | Phase 1 + 直接 Phase 5 | 跳过大部分 checkpoint |
| 修复显而易见的 bug | 直接 Phase 5 | 跳过所有推理 |
| 添加新文件/模块 | Phase 1-2 + 直接 Phase 5 | 新 entity 无下游，但需确认不与现有代码冲突 |

`cog next` 的建议引擎会根据 entity 特征（downstream 数量、断言覆盖度、kind）推荐适当的下降深度——见 [architecture/06-workflow-layer.md](../architecture/06-workflow-layer.md)。

## 5 为什么是两个状态变量，不是一个

下降协议引入 experiment 状态后，直觉做法是把 experiment 状态加入 `WorkflowState` 枚举。这在具体场景下崩溃了：

```
1. cog experiment start SeqBuilder     → "Experimenting"
2. cog experiment evaluate <id>        → "ExperimentEvaluated"
3. 读代码验证假设（此时 workflow 应是 Exploring）
4. agent 改代码（sync 检测到 drift → PostChange）
```

第 3-4 步 agent 同时处于"潜空间有一个已完成推演的 experiment"和"代码空间正在验证或已变更"。这两个状态不互斥——一个在潜空间，一个在代码空间，它们应该同时存在。

cog 的解法是使用两个独立状态变量：

- `WorkflowState`（追踪代码空间）：`Uninit` | `Ready { phase }`。见 [architecture/06-workflow-layer.md](../architecture/06-workflow-layer.md)。
- experiment 状态（追踪潜空间）：`Open` → `Evaluated` → `Committed`/`Discarded`。见 [architecture/07-experiment-layer.md](../architecture/07-experiment-layer.md)。

`cog next` 读取两者，通过两阶段决策（先 experiment 优先级，再 phase 特定建议，再停滞检测）合成建议，而非笛卡尔积。代码空间变更（sync drift → PostChange）不改变 experiment 状态，进入/退出 experiment 也不改变 workflow phase——除了一个例外：`experiment commit` 会将 phase 设为 `PendingImplement`，因为模型已更新但代码尚未跟上。
