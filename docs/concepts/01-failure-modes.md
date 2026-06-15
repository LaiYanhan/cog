# 失败模式：潜空间与代码空间的 gap

> 本文基于 SWE-CI benchmark 的真实任务轨迹分析，归纳 cog 在"从模型推理到代码实现"过程中失败的三种模式。理解这些失败模式是理解 [下降协议](02-descent-protocol.md) 为何必要的前提。
>
> 来源：2026-06 的 10-task pilot 分析。案例数据是历史快照，归纳的失败模式至今仍成立。

## 1 根本问题：fidelity gap

cog 构建了一个**潜空间**——对代码的压缩、抽象表征。压缩本身是核心价值（见 [vision/03-latent-space.md](../vision/03-latent-space.md) §2）。**但 agent 最终必须在代码空间执行变更**。从潜空间的抽象推理回到代码空间的具象实现，存在根本性的 fidelity gap。

这不是 prompt 设计问题，不是 agent 能力问题，而是**信息论约束**：任何抽象都会丢失信息，丢失的信息在还原时必然成为未知。关键不在于消除 gap——这不可能——而在于**设计一个能在 gap 暴露时安全降落的机制**（即下降协议）。

## 2 三种失败模式

### 2.1 抽象坍塌（Abstraction Collapse）

> 潜空间的推理正确，但代码空间的实现细节不在模型中。

**特征**：`cog impact` 返回低风险，agent 信心充足，实现时撞上未建模的约束。

**典型案例（dbrattli）**：

| 维度 | 数据 |
|------|------|
| Entity | `SeqBuilder`（type） |
| `cog impact` 结论 | "Low risk (0.10), 0 downstream, 2 assertions" |
| 实际风险 | Python generator protocol（`yield-from` 语义） |
| 结果 | 8 个 epoch 在 4 种实现间振荡，42.6% 的工具调用是 cog 命令 |

`SeqBuilder.__call__` 涉及 Python generator protocol（`__next__`、`send()`、`yield-from`、`StopIteration`），这些都是运行时行为，不在结构依赖图中。agent 基于模型做了正确推理（"0 dependents, safe to change"），但实现需要处理完整的运行时协议。

**根因**：结构压缩丢失了控制流和运行时行为，而这些恰好是修改实现时最需要的信息。

### 2.2 级联发现（Cascading Discovery）

> 实现过程中发现新的约束，这些约束颠覆了推理前提。

**特征**：初始变更触发预期外的级联效应，需要重做大量推理。

**典型案例（argoproj）**：

| 维度 | 数据 |
|------|------|
| 初始 gap | 64 |
| Epoch 2 行为 | 5 次 `cog impact` 后大范围修改 3 个文件 |
| 结果 | gap 从 55 飙升至 70（+15 个新失败） |
| 最终 | 15 epochs 解决（baseline 20 epochs 未解决） |

修改 3 个文件的计划在潜空间是正确的。但实现时对 `pydantic` validation 和 `argo-workflows` SDK 的隐式约束（必须同时修改 model definition 和 YAML template）缺乏了解。这些约束既不在 entity graph 也不在 assertion 中。agent 执行了计划的前半部分（改 model），没执行后半部分（改 template），因为不知道后半部分存在。

**根因**：依赖图只覆盖语言级关系（import、调用），不覆盖框架级、配置级的隐式依赖。

### 2.3 沉没成本陷阱（Sunk Cost Trap）

> Agent 在潜空间投入大量推理，形成的断言在实现失败后未被清理，成为下次推理的噪音。

**特征**：模型中有大量"计划做什么"的断言，实现失败后未被 retract，残留为噪音。

**典型案例（dbrattli）**：agent 对 `SeqBuilder.__call__` 做了 12 次 retract——每次尝试一个实现方案，先 assert 它是对的，实现后发现不对，再 retract。模型变成了 agent 错误的历史记录。

**根因**：模型的写操作立即可见，但没有自动清理推理轨迹的机制。虽然 retract 标记了 assertion 为废弃，但实践中高频试错会产生大量噪音断言。

## 3 正面案例：当推理与实现一致时

**amaranth**：

| 维度 | 数据 |
|------|------|
| 核心决策 | 将 Signal 类从 `__init__` 改为 `__new__` + `__init__` 分割 |
| `cog impact` | 显示 4 downstream，risk 0.30 |
| 关键行为 | 97 次 bash smoke test（baseline 仅 17 次） |
| 结果 | 4 epochs 解决，零回归 |

`__new__/__init__` 分割是**静态结构变更**——正好在 cog 的扫描能力范围内。潜空间推理与代码空间实现的 gap 很小。加上 agent 做了 97 次增量测试验证每次小改动——用高频反馈弥补了 gap。

**启示**：cog 在"静态结构变更"场景下最可靠。当变更涉及运行时协议或框架隐式约束时，必须靠频繁的代码空间反馈（smoke test）来弥补，而非依赖潜空间推理。

## 4 第四种失败：分析瘫痪

> Agent 在潜空间中无限探索，没有试错代价，迟迟不进入代码空间。

**案例（9001__co）**：

| 维度 | 数据 |
|------|------|
| Epoch 11 | 128K tokens，140 工具调用，0 edits |
| cog 命令 | 15 次（query x4, impact x3, next x2, assert x3, verify, stats, export） |
| epoch 6-13 | gap=1 卡了 8 个 epoch，114 次 cog 命令 |

agent 反复在潜空间探索（query→impact→assert→retract），每次试错只在认知模型中调整，没有真正去代码空间做最小变更。潜空间变成了舒适的沙盒。

**启示**：这正是 `cog next` 停滞检测要解决的问题——检测到长时间停留在只读循环时，催促 agent 进入实现。见 [architecture/06-workflow-layer.md](../architecture/06-workflow-layer.md) 的停滞检测机制。
