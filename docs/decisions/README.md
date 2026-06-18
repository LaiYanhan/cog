# 决策记录（ADR）

> 架构决策记录（Architecture Decision Records）。每条记录一个不可逆或影响深远的决策及其**理由**。append-only——决策被推翻时新增一条引用旧条目的记录，不修改旧条目。
>
> 格式：**决策** → **选择** → **理由**。理由是这些记录的价值所在——它们让未来的人不必重新推导。

## 索引

按主题分组。括号为状态：生效 / 被后续取代 / 未实现。

### 持久化

| # | 决策 | 状态 |
|---|------|------|
| [ADR-01](#adr-01-repository-用-trait) | Repository 用 trait | 生效 |
| [ADR-02](#adr-02-测试用-sqlite-memory) | 测试用 SQLite `:memory:` | 生效 |
| [ADR-03](#adr-03-sqlite-子模块拆分) | sqlite 按领域概念拆 8 子模块 | 生效 |
| [ADR-04](#adr-04-迁移只允许加性变更) | 迁移只加不删 | 生效 |

### 图算法

| # | 决策 | 状态 |
|---|------|------|
| [ADR-05](#adr-05-撤销的真实执行-vs-模拟) | 模拟与执行分离 | 生效 |
| [ADR-06](#adr-06-groundweakened-非存储状态) | `GroundWeakened` 非存储状态 | 生效 |
| [ADR-07](#adr-07-impact-只跟随-callsuses) | impact 排除 Contains | 生效 |

### 类型设计

| # | 决策 | 状态 |
|---|------|------|
| [ADR-08](#adr-08-放弃-newtype) | 放弃 newtype，用自由函数 | 生效 |
| [ADR-09](#adr-09-renderable-而非-display) | `Serialize` + `Renderable` + TextRenderer | 生效 |
| [ADR-10](#adr-10-anyhow-错误处理) | anyhow | 生效 |

### 工作流

| # | 决策 | 状态 |
|---|------|------|
| [ADR-11](#adr-11-cli-工作流不用-typestate) | 序列化 enum | 生效 |
| [ADR-12](#adr-12-移除-changing-状态) | 状态转换自动化 | 取代早期设计 |
| [ADR-13](#adr-13-init-合并为-sync) | `sync --init` | 取代 `init` |
| [ADR-14](#adr-14-next-两阶段决策) | 两阶段而非笛卡尔积 | 生效 |

### Experiment / Branch

| # | 决策 | 状态 |
|---|------|------|
| [ADR-15](#adr-15-commit-用-replay) | replay 操作日志 | 生效 |
| [ADR-16](#adr-16-branch-降级为-backup) | Experiment 推理 + Backup 全量 | 生效 |
| [ADR-17](#adr-17-draftsave-语义) | 自动持久化 draft + save 标记 | 生效 |

### CLI 设计

| # | 决策 | 状态 |
|---|------|------|
| [ADR-18](#adr-18-has_drift-结构体字段) | `has_drift` 字段非文本解析 | 生效 |
| [ADR-19](#adr-19-未实现的-cli-设计) | next --verbose / 前缀等 | 未实现 |
| [ADR-20](#adr-20-停滞检测只看写操作) | changelog 只记写操作 | 生效 |

---

## 持久化

### ADR-01 Repository 用 trait

**选择**：trait。**理由**：全系统唯一真正需要多态的层——experiment 的模拟需要不触碰真实 DB，测试需要可替换实现。其它层（command、space、format）不需要 trait，不为抽象而抽象。

### ADR-02 测试用 SQLite `:memory:`

**选择**：SQLite `:memory:`。**理由**：`HashMap` 无法忠实模拟 FOREIGN KEY 约束、事务隔离、级联删除。`:memory:` 保留 100% SQL 保真度，零磁盘 I/O。不实现 `InMemoryRepository`。

### ADR-03 sqlite 子模块拆分

**选择**：按领域概念拆 8 子模块（entities/assertions/evidence/relations/changelog/stats/helpers + 模块声明）。**理由**：原 `Store` 是 1200+ 行 God Object，拆分后每个子模块聚焦单一概念，可独立理解与测试。

### ADR-04 迁移只允许加性变更

**选择**：`ADD COLUMN`/`CREATE TABLE`，禁止 `DROP`。**理由**：cog 的 `.cog/cog.db` 跨 session 积累知识，破坏性迁移等于丢失不可重建的认知。见 AGENTS.md 数据保全条款。

## 图算法

### ADR-05 撤销的真实执行 vs 模拟

**选择**：分离——`SemanticSpace::simulate_retract`（纯内存，experiment 用）+ `CascadeEngine::retract`（真实 DB，CLI 用）。**理由**：experiment 需要轻量可丢弃的推演；CLI `retract` 需要"独立 Active 依赖"检查的实时全局状态，局部子图模拟会误判。见 [reference/03-tms-cascade.md](../reference/03-tms-cascade.md)。

### ADR-06 `GroundWeakened` 非存储状态

**选择**：存储状态仅 3 种（Active/Retracted/Uncertain）；`GroundWeakened` 是 `CascadeReason`，断言保持 Active。**理由**：二态模型在实践中已足够——有其它支撑的断言确实仍有效，无需新状态。DB CHECK 约束强制。这是对早期文档把 `GroundWeakened` 描述为第四种状态的纠正。

### ADR-07 impact 只跟随 Calls+Uses

**选择**：BFS 排除 `Contains`。**理由**：`Contains` 是结构性组合（模块包含函数），不是依赖——改一个函数不需要改包含它的模块。

## 类型设计

### ADR-08 放弃 newtype

**选择**：不采用 newtype（QualifiedName/EntityId），用自由函数（`last_segment`/`parent_qname`/`ancestors` 集中在 `domain/naming.rs`）。**理由**：Rust trait-object + coherence 限制下，newtype 迁移需改 `HashSet`/`HashMap`/`Repository` trait 全量签名，成本远超收益。

### ADR-09 Renderable 而非 Display

**选择**：`Serialize` + `Renderable` trait + `TextRenderer`（文本）。**理由**：`Display` 锁死单一输出格式。`Renderable` 使加 `--output json`（`serde_json` 内联于 `emit_report`）是加法，不改动所有命令。

### ADR-10 anyhow 错误处理

**选择**：`anyhow`。**理由**：CLI 工具不需要细粒度错误匹配。`bail!` 处理前置失败，`CommandOutput::with_exit_code` 处理非错误退出（如 entity 未找到是 exit 1 但非 panic）。

## 工作流

### ADR-11 CLI 工作流不用 typestate

**选择**：序列化 enum + 运行时建议引擎。**理由**：CLI 每次调用是新进程，typestate 的编译期保证无法跨进程传递。

### ADR-12 移除 Changing 状态

**选择**：移除 `Changing` 顶级状态与 `start-change`/`finish-change`/`abort-change` 命令；状态转换由命令隐式触发。**理由**：手动状态声明增加认知负担且易与实际脱节。sync 检测 drift 自动进 PostChange，retract 自动进 Debugging，verify 通过自动退出。**取代了** RUST_ARCHITECTURE_REDESIGN §3.4 的 Changing 设计。

### ADR-13 init 合并为 sync

**选择**：单一 `cog sync`（`--init` 首次创建），合并旧的 `init` + `sync`。**理由**：两者跑同一 AST 扫描，`upsert_entity` 幂等，区别纯是 CLI 层。合并消除"何时用 init 何时用 sync"的认知负担，且避免半同步状态。**取代了** CLI_V2 §7.3 的两步流程。

### ADR-14 next 两阶段决策

**选择**：阶段 1 experiment 优先级（phase-independent）+ 阶段 2 phase 特定 + 阶段 3 停滞检测。**理由**：多并行实验使 `(phase, experiment_status)` 笛卡尔积爆炸到 15+ 组合。两阶段让两者独立贡献建议，自动支持多并行实验。**取代了** CLI_V2 §7.2 的笛卡尔积设计。

## Experiment / Branch

### ADR-15 commit 用 replay

**选择**：replay 操作日志。**理由**：确定性回放，不需要 UUID 冲突解决。比 diff-then-merge 简单可靠。

### ADR-16 branch 降级为 backup

**选择**：Experiment 做单根假设推理，Branch 降级为全量 backup（VACUUM INTO）。**理由**：原 Branch 的 merge 语义薄弱、UUID 共享导致合并歧义、开销不对称。日常推理走 Experiment，大规模变更前用 Backup 做安全网。

### ADR-17 draft/save 语义

**选择**：自动持久化 draft（`saved: false`）+ `save` 标记 checkpoint（`saved: true`）。**理由**：避免丢失未保存工作；`list` 区分 draft/saved。

## CLI 设计

### ADR-18 has_drift 结构体字段

**选择**：`CommandOutput.has_drift: bool`，由 `SyncReport.has_drift` 设置，cli 直接读。**理由**：曾用 `!out.text.contains("no drift")` 从格式化文本推断状态——格式文本变化会致状态转换悄悄失效。结构体字段是带外信号。见 [architecture/08-output-layer.md](../architecture/08-output-layer.md)。

### ADR-19 未实现的 CLI 设计

**选择**：以下 CLI V2 设计项**未实现**（理由见各条）：

- **`next --verbose` 合并 stats**：next 输出已含 LLM 决策所需的全部统计维度。`stats` 保留为独立命令供脚本。
- **`impact --verbose`**：默认输出已含 covered/blind + risk。深信息应直接 `query`。
- **`[COG:<command>]` 输出前缀**：对 LLM 无额外信息，徒增每条 ~12 token。
- **`index --top`**：与 impact 职责重叠。
- **`sync --clean`**：孤立清理是 verify 的职责，sync 只管 drift。

### ADR-20 停滞检测只看写操作

**选择**：changelog 只记录写操作（Assert/Retract/Depend/Sync/Verify），停滞检测基于此。**理由**：为检测"反复 query 但不 assert"需为只读操作写 changelog——DB 写入量增 2-5 倍/epoch，changelog 膨胀。真正的停滞（verify 循环、experiment stale）已能由写操作检测。
