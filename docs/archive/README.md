# 历史归档

本目录存放 2026-06 的四份原始设计文档。它们已被 [`docs/`](../README.md) 下的结构化文档系统**取代**，保留于此仅供追溯设计脉络。

## 不要在此修改

这些文档是**时间快照**，反映写作时刻的设计意图，其中部分内容已被后续实现证伪。要更新请写新文档，不要改动归档。各文档与当前实现的偏差已在对应新文档中标注。

## 后继映射

| 归档文档 | 内容去向 |
|---------|---------|
| `COGNITIVE_MODEL_DESIGN.md`（2026-06-03） | 理论 → [vision/](../vision/)；价值分析 → [vision/03-latent-space.md](../vision/03-latent-space.md) |
| `RUST_ARCHITECTURE_REDESIGN.md`（2026-06-06） | 分层架构 → [architecture/](../architecture/)；设计原则与决策表 → [decisions/](../decisions/README.md) |
| `LATENT_TO_CODE_MAPPING.md`（2026-06-08） | 失败模式 → [concepts/01-failure-modes.md](../concepts/01-failure-modes.md)；下降协议 → [concepts/02-descent-protocol.md](../concepts/02-descent-protocol.md) |
| `CLI_INTERFACE_V2.md`（2026-06-08） | 命令规格 → [reference/01-cli-reference.md](../reference/01-cli-reference.md)；设计原则 → 该文档§1；偏差说明 → [decisions/](../decisions/README.md) |

## 已被实现证伪的关键内容

以下是四份文档中**与当前代码不符**的典型条目（非完整清单），新文档已修正：

- **`GroundWeakened` 不是存储状态**：归档文档将其描述为 `AssertionStatus` 的第四种值。实际代码中存储状态只有 `Active`/`Retracted`/`Uncertain` 三种；`GroundWeakened` 仅为级联报告中的 `CascadeReason`，断言仍保持 `Active`。见 [reference/03-tms-cascade.md](../reference/03-tms-cascade.md)。
- **`WorkflowState` 无 `Changing` 变体**：归档描述的 `start-change`/`finish-change`/`abort-change` 命令及 `Changing` 状态已被移除。当前只有 `Uninit` 与 `Ready { phase }`。见 [architecture/06-workflow-layer.md](../architecture/06-workflow-layer.md)。
- **`WorkflowPhase` 无 `Assessing`**：当前五相位为 `FreshScan`/`Exploring`/`PendingImplement`/`PostChange`/`Debugging`。
- **`cog init` 已合并为 `cog sync --init`**：扫描命令现在是单一幂等的 `sync`。见 [reference/01-cli-reference.md](../reference/01-cli-reference.md)。
- **`EntityOrigin` 为三种**：`Manual`/`Scan`/`Experiment`（归档仅描述前两种）。
- **`retract` 命令接受 `&dyn Repository`**：归档与 `AGENTS.md` 早期版本称其需要 `&SqliteRepository`（因 `transaction()`），实际现已无需。仅 `sync` 因事务需要具体类型。
