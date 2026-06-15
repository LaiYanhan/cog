# 潜空间：价值与定位

> 认知模型本质上是将原始代码空间压缩为一个可推理的**潜空间**（latent space）。本文阐述这个潜空间的层次、不可替代价值、以及诚实的局限。

## 1 潜空间的四个层次

| 层次 | 来源 | 信息类型 | 构建 | 类比 |
|------|------|----------|------|------|
| 结构层 | tree-sitter 扫描 | entity 名称、kind、containment 层级 | `cog sync`（自动） | 骨架 |
| 关系层 | import/call 解析 + manual depend | contains/calls/uses 有向边 | `cog sync` + `cog depend` | 连接 |
| 语义层 | agent assert | contract/intent/invariant/fragility/correction | `cog assert` | 理解 |
| 时序层 | changelog | 操作审计轨迹 | 所有写命令自动追加 | 历史 |

前两层构成 [02-cognitive-model.md](02-cognitive-model.md) §5 的"自动层"，第三层是"理解层"。时序层是横切关注点——`cog next` 的停滞检测依赖它。

## 2 不可替代价值

一个自然的问题：随着基础模型能力增强，直接读代码就能完成所有分析，cog 的价值何在？答案在于三个基础模型做不到的特性：

**推理持久化。** cog 存的不是事实（"merge_branch 存在于 branch_cmd.rs，54 行，调用 apply_item"），而是推理链——intent、fragility、correction、设计决策。推理链携带因果结构，agent 读到断言时不需要重新理解"为什么这个改过"。

**TMS 级联。** 当知识被推翻时（retract），系统沿 `depends_on` 边 BFS，自动识别所有受影响的推理。没有任何静态分析工具或基础模型能在 session 之间做到这件事。TMS 级联将"知识变动的影响评估"从非确定性的推理问题变成了确定性的图算法。见 [reference/03-tms-cascade.md](../reference/03-tms-cascade.md)。

**潜空间将规划变成图搜索。** 没有 cog 时，"如果我要改 Store，会影响什么？"是一个需要逐文件阅读的推理问题。有了 cog 后，它变成 `cog impact model::store::Store`——一个 O(V+E) 的 BFS 遍历，与代码库大小线性相关，与 agent 的注意力窗口无关。

## 3 两次信息压缩

潜空间的构建经过两次有损压缩——这是 [下降协议](../concepts/02-descent-protocol.md) 要应对的根本问题的根源：

| 压缩 | 输入 | 输出 | 丢失的信息 |
|------|------|------|-----------|
| **结构压缩** | 源码文本（AST） | entity name + kind + contains/calls/uses | 控制流、变量生命周期、异常处理、并发语义 |
| **语义压缩** | agent 对代码的理解 | assertion 文本 | 隐含假设、边界条件、上下文依赖、性能特征 |

压缩本身是 cog 的核心价值——"潜空间将规划变成图搜索"。但 agent 最终必须在代码空间执行变更，从抽象推理回到具象实现存在根本性的 fidelity gap。压缩丢失的信息在推理阶段隐藏，在实现阶段暴露。

## 4 当前局限

**实体元数据稀疏。** tree-sitter 扫描提取名称、kind、行数、fan-in/out。`Store::stats`（重构方法）和 `Store::get_entity`（简单查询）在结构信号上差异有限。缺少实现复杂度的更细粒度信号。

**查询返回数据而非推理。** `cog query X` 返回 X 的断言和关系列表。agent 真正想问的是"改 X 安全吗？"或"X 有什么历史教训？"。这些综合判断仍需 agent 组合多个命令。

**结构图不捕获运行时行为。** `cog impact` 返回 `downstream: 0` 不等于"可以放心改"——运行时协议（如 Python generator 的 yield-from）、并发约束、框架级隐式依赖都不在结构依赖图中。这是真实任务中最危险的失败模式，见 [concepts/01-failure-modes.md](../concepts/01-failure-modes.md)。

**断言间 `depends_on` 使用率低。** 即使有完整的 `--depends-on` 支持，实践中 agent 极少建立断言间的依赖链。这意味着 TMS 级联在真实使用中常因缺少 `depends_on` 边而无法充分传播——级联更多发生在显式 retract 时，而非依赖推理被推翻时。

## 5 与人类认知的类比

模型随实践渐进积累，与人类加入新团队的过程一致：

```
Session 1: 模型为空 → 处理第一个 task → 构建涉及的 Entity 和 Assertion
Session 2: 模型有少量内容 → 部分复用，新增，可能修正已有 Assertion
Week 2:   模型逐渐丰满 → 核心模块已有完整认知 → 新 task 大量复用
```

cog 不追求一次性建完备的模型，而是追求"模型只增不减地积累，且每次积累都可追溯"。这是它作为长期记忆而非一次性分析工具的定位。
