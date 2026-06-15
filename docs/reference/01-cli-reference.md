# CLI 参考

> 全部命令、全部 flag、精确语义。机械查阅文档，不含叙述。命令清单源自 `src/cli.rs` + `src/cli/args.rs`。

## 全局选项

| 选项 | 说明 |
|------|------|
| `--db <path>` / `COG_DB` | 指定模型 DB 路径。默认从 CWD 向上查找 `.cog/cog.db` |
| `--output text|json` | 输出格式（默认 text）。全局 flag |

DB 定位规则（`main.rs`）：`sync --init` 总在 `<CWD>/.cog/cog.db` 创建；否则向上查找现有 `.cog/cog.db`；否则用显式 `--db`；都没有则报错引导 `cog sync --init`。

## 命令清单（16 个）

### 写入模型

#### `cog sync` —— 代码空间同步

幂等全量扫描，创建 entity + relation，清理 stale `Scan` entity（保护有 assertion 的）。

```
cog sync --init                  # 首次：创建 .cog/ 再扫描（Uninit → FreshScan）
cog sync                         # 扫描（需已存在 .cog/）
cog sync --lang python,rust      # 仅扫描指定语言
cog sync --dry-run               # 预览不写入
```

| Flag | 说明 |
|------|------|
| `--init` | 创建新模型后再扫描 |
| `--dry-run` | 只预览，不写 DB |
| `--lang <csv>` | 逗号分隔语言过滤 |

输出含 `has_drift` 信号（驱动 workflow 转换）：检测到 drift → `PostChange`。sync 合并了旧的 `init`——两者跑同一 AST 扫描，`upsert_entity` 幂等，区别纯是 CLI 层。

#### `cog assert` —— 记录断言

```
cog assert <entity> --kind <kind> --claim "<claim>" --grounds "<source:detail>" [--depends-on <id>] [--replace <id> | --force]
```

| Flag | 说明 |
|------|------|
| `--kind` | `contract`/`intent`/`invariant`/`fragility`/`correction`（必填） |
| `--claim` | 自然语言声明（必填） |
| `--grounds` | `source:detail` 格式证据（必填） |
| `--depends-on <id>` | 依赖的另一 assertion ID（短/全） |
| `--replace <id>` | 先 retract 指定 assertion 再创建（与 `--force` 互斥） |
| `--force` | 允许与现有同 kind active 断言并存（覆盖不同方面，与 `--replace` 互斥） |

实体不存在时自动创建 `Manual` origin entity。`--replace`/`--force` 是同 kind 重复断言的门控。

#### `cog retract` —— 撤回断言（TMS 级联）

```
cog retract <id> --reason "<why>"
```

撤回指定 assertion，BFS 级联传播 `Uncertain`。输出含级联影响清单 + 该 entity 剩余断言（状态变化标记）。精确算法见 [03-tms-cascade.md](03-tms-cascade.md)。

#### `cog depend` —— 记录实体关系

```
cog depend <entity_a> --on <entity_b> --kind <contains|calls|uses>
```

#### `cog delete-entity` —— 删除实体

```
cog delete-entity <qualified_name>
```

级联删除该 entity 的所有 assertion、evidence、relation。

### 读取模型

#### `cog query` —— 实体认知卡片

```
cog query <entity> [--all] [--compact] [--relations|-r]
```

| Flag | 说明 |
|------|------|
| `--all` | 含 retracted 断言（默认仅 active） |
| `--compact` | 紧凑模式，每断言一行，无 evidence/relation |
| `--relations`/`-r` | 完整关系列表（默认按 kind 摘要） |

#### `cog impact` —— 下游影响面

```
cog impact <entity>
```

BFS 通过 `Calls`+`Uses` 反向边找下游（`Contains` 排除）。输出含每下游实体的 assertion 数、covered/blind 标记、risk score。

#### `cog trace` —— 依赖链追溯

```
cog trace <entity>
```

DFS 构建断言依赖链 + evidence + 实体关系。

#### `cog index` —— 实体索引

```
cog index [--kind <k>] [--origin <o>] [--prefix <p>] [--verbose] [--uncovered]
```

| Flag | 说明 |
|------|------|
| `--kind` | 过滤 module/function/type/field/method |
| `--origin` | 过滤 manual/scan/experiment |
| `--prefix` | qualified name 前缀过滤 |
| `--verbose` | 完整列表（默认是覆盖摘要） |
| `--uncovered` | 仅显示无 assertion 的实体 |

#### `cog stats` —— 模型统计

```
cog stats
```

#### `cog verify` —— 结构一致性检查

```
cog verify [--scope <prefix>] [--clean] [--scan] [--scan-path <path>]
```

| Flag | 说明 |
|------|------|
| `--scope` | 限定到某前缀子树 |
| `--clean` | 自动删除孤立 entity（保护 Uncertain 断言） |
| `--scan` | 与实际代码对比，检测 stale/unmodeled |
| `--scan-path` | 扫描路径（配合 `--scan`，默认 `.`） |

检查维度：`IsolatedEntity`、`MissingEvidence`、`DependencyOnRetracted`、`DependencyOnUncertain`、`DanglingGrounds`、`OrphanManualEntity`。

#### `cog export` —— 模型导出

```
cog export --format <json|toml|dot>
```

#### `cog recover` —— 恢复 Uncertain 断言

```
cog recover [--apply]
```

列出（或 `--apply` 恢复）所有依赖已重新变为 Active 的 `Uncertain` 断言。仍被阻塞的单独列出。

#### `cog next` —— 建议下一步

```
cog next
```

读取 workflow state + stats + 活跃实验，输出状态摘要 + 建议列表。是 agent 与 cog 交互的单一引导点。见 [architecture/06-workflow-layer.md](../architecture/06-workflow-layer.md)。

### 实验

```
cog experiment <subcommand>
```

| 子命令 | 说明 |
|--------|------|
| `try <entity> --kind .. --claim .. --grounds .. [--desc ..] [--depends-on ..]` | 一步完成 start+hypothesize+evaluate |
| `start <entity> [--description "<desc>"] [--max-nodes <n>]` | 开始实验（默认 500 节点子图） |
| `hypothesize <id> --entity <entity> --kind .. --claim .. --grounds ..` | 注入假设断言 |
| `hypothetical-relation --id <id> --from <a> --to <b> --kind ..` | 注入假设实体关系 |
| `hypothetical-delete --id <id> --entity <entity>` | 注入假设实体删除 |
| `evaluate <id>` | 评估 staged 操作影响 |
| `report <id>` | 显示完整报告 |
| `commit <id>` | 回放 staged 操作到真实模型 |
| `discard <id>` | 丢弃实验 |
| `list` | 列出全部实验（区分 draft/saved） |
| `save <id>` | 标记为 checkpoint |
| `load <id>` | 加载已保存实验 |

生命周期语义见 [architecture/07-experiment-layer.md](../architecture/07-experiment-layer.md)。

### 备份

```
cog backup create --name <name>     # VACUUM INTO 全量快照
cog backup list
cog backup restore <name>           # 恢复（先 checkpoint WAL）
cog backup drop <name>
```

保留名：`_main`、`_main_backup`。

## 设计原则

四条指导 CLI 设计的原则（源自 CLI V2，至今有效）：

1. **注意力经济**——默认输出精简（"恰好足够"），细节通过 flag 按需展开。token 开销是每 epoch 累计成本。
2. **即时上下文**——写操作（assert/retract/depend）返回受影响 entity 的当前完整状态，agent 无需 extra query"回头看"。
3. **下降感知**——每条命令有明确的下降阶段定位（SURVEY/HYPOTHESIZE/SCOUT/PROBE/COMPLETE）。
4. **单一引导点**——agent 只需知道 `cog next`，其余命令由其建议引出。

## 未实现/已移除的设计

（源自 CLI V2 的"偏差说明"，已并入决策记录——见 [../decisions/README.md](../decisions/README.md)）

- `next --verbose` 合并 stats：未实现，`stats` 保留为独立命令。
- `impact --verbose`：未实现，默认输出已含 covered/blind + risk。
- `[COG:<command>]` 输出前缀：未实现，前缀对 LLM 无额外信息。
- 只读循环停滞检测：未实现（changelog 只记录写操作）。
- `index --top`：未实现（与 impact 职责重叠）。
- `sync --clean`：未实现（孤立清理是 verify 的职责）。
- `init` 独立命令：已合并为 `sync --init`。
- `start-change`/`finish-change`/`abort-change`：已移除（状态转换自动化）。
