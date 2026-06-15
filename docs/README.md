# Cog 文档系统

本目录是 cog 项目的**正式文档**。它按"变更频率"分层组织，每一层都有明确的**维护契约**——改了哪部分代码，就必须更新哪份文档。文档与代码同等重要，违反契约等同于留下过时注释。

> 运行时契约（agent 如何使用 cog）不在本目录，见根目录 [`AGENTS.md`](../AGENTS.md) 与 [`skills/cog/`](../skills/cog/)。本目录只讲**为什么这样设计**和**代码如何实现**。

---

## 文档地图

按阅读目的选择入口：

| 我想…… | 读哪里 |
|--------|--------|
| 理解 cog 解决什么问题 | [vision/01-problem-and-thesis.md](vision/01-problem-and-thesis.md) |
| 掌握核心概念（Entity/Assertion/TMS） | [vision/02-cognitive-model.md](vision/02-cognitive-model.md) |
| 理解"潜空间"的价值定位 | [vision/03-latent-space.md](vision/03-latent-space.md) |
| 知道 cog 在真实任务中如何失败 | [concepts/01-failure-modes.md](concepts/01-failure-modes.md) |
| 学习"下降协议"工作流 | [concepts/02-descent-protocol.md](concepts/02-descent-protocol.md) |
| 看整体架构与模块地图 | [architecture/01-overview.md](architecture/01-overview.md) |
| 深入某一层代码 | [architecture/](architecture/) 下对应文件 |
| 查某个命令/flag 的精确语义 | [reference/01-cli-reference.md](reference/01-cli-reference.md) |
| 查数据库 schema 或类型定义 | [reference/02-data-model.md](reference/02-data-model.md) |
| 理解级联撤回的精确算法 | [reference/03-tms-cascade.md](reference/03-tms-cascade.md) |
| 查某个设计决策的理由 | [decisions/](decisions/README.md) |
| 查历史设计文档 | [archive/](archive/README.md)（已被取代，仅留脉络） |

---

## 分层与维护契约

### `vision/` —— 为什么（稳定层）

阐述 cog 的问题定义、理论框架与价值定位。这些内容反映**持久的设计思想**，与具体代码实现解耦，变更频率极低。

**契约**：仅当项目的核心论点或理论基础发生根本性转变时才更新。重构、新增命令、schema 变更**不**触发本层修改。

### `concepts/` —— 怎么用（概念层）

下降协议与失败模式分析——指导 agent 如何安全地从模型推理下降到代码实现。

**契约**：当工作流模型（`workflow/`）或 experiment 生命周期的**语义**发生改变时更新。日常命令调整不触发。

### `architecture/` —— 怎么实现（架构层，**维护最频繁**）

每个文件对应代码的一个分层。这是本系统的核心：**locality of change**——改了 `repo/` 就只更新 [03-persistence-layer.md](architecture/03-persistence-layer.md)，改了 `workflow/` 就只更新 [06-workflow-layer.md](architecture/06-workflow-layer.md)。

| 文档 | 跟踪的代码 |
|------|-----------|
| [01-overview.md](architecture/01-overview.md) | 整体分层、依赖方向、`src/main.rs`、`src/cli.rs` |
| [02-domain-layer.md](architecture/02-domain-layer.md) | `src/domain/` |
| [03-persistence-layer.md](architecture/03-persistence-layer.md) | `src/repo/`（含 schema 定义） |
| [04-analysis-layer.md](architecture/04-analysis-layer.md) | `src/analysis/` |
| [05-space-layer.md](architecture/05-space-layer.md) | `src/space/` |
| [06-workflow-layer.md](architecture/06-workflow-layer.md) | `src/workflow/`、`src/command/next_cmd.rs` |
| [07-experiment-layer.md](architecture/07-experiment-layer.md) | `src/experiment/`、`src/command/experiment_cmd.rs` |
| [08-output-layer.md](architecture/08-output-layer.md) | `src/format/`、`CommandOutput` |

**契约**：修改上述任一代码区域时，**必须**同步更新对应文档。这是"代码库严格标准"的强制项。

### `reference/` —— 精确参考（机械层）

面向查阅的精确规格：CLI 接口、数据模型、算法语义。不含叙述，只含事实。

**契约**：新增/修改命令 flag、修改 schema、修改级联逻辑时更新。

### `decisions/` —— 决策记录（ADR，append-only）

架构决策记录（Architecture Decision Records）。每条记录一个不可逆或影响深远的决策及其理由。**只追加，不修改**——决策被推翻时新增一条引用旧条目的记录。

**契约**：做出新的重大架构决策时追加；重构不删除既有记录。

### `archive/` —— 历史归档

2026-06 的四份原始设计文档，已被本系统取代。保留是为了让未来的读者能追溯设计脉络。**不要**在这里修改或补充——要更新请写新文档。

---

## 文档规范

- **语言**：中文叙述 + 英文技术术语（类型名、命令名、字段名保持原文）。与既有设计文档风格一致。
- **代码引用**：用反引号标注路径/符号，如 `src/repo/trait.rs`、`CascadeEngine::retract`。
- **交叉引用**：使用相对路径 markdown 链接。新增文档后必须能从本 README 或其所属分层的入口到达。
- **准确性优先**：宁可写"截至 vX 的实现"，也不要写模糊的"通常"。代码是事实来源；文档描述代码，不规定代码。
- **不要复制代码**：引用签名，描述职责与不变量，不粘贴大段实现。读者会去看源码。
- **过时即修**：发现文档与代码不符是 bug，当场修掉或在 decisions/ 记录偏差。
