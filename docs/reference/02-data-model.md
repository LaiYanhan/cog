# 数据模型参考

> 数据库 schema + 领域类型 + ID/命名约定。源自 `src/repo/sqlite/helpers.rs::SCHEMA` 与 `src/domain/`。

## Schema（6 表 + 8 索引）

### `entities`

| 列 | 类型 | 说明 |
|----|------|------|
| `id` | TEXT PRIMARY KEY | UUID v4 |
| `qualified_name` | TEXT UNIQUE NOT NULL | `::` 分隔路径 |
| `kind` | TEXT NOT NULL | module/function/type/field/method/unknown |
| `origin` | TEXT NOT NULL DEFAULT 'manual' | manual/scan/experiment |
| `metrics_json` | TEXT | 序列化的 `EntityMetrics`（可空） |
| `created_at` | TEXT NOT NULL | RFC3339 |

### `assertions`

| 列 | 类型 | 说明 |
|----|------|------|
| `id` | TEXT PRIMARY KEY | UUID v4 |
| `entity_id` | TEXT NOT NULL REFERENCES entities(id) | 归属实体 |
| `kind` | TEXT NOT NULL | contract/intent/invariant/fragility/correction |
| `claim` | TEXT NOT NULL | 自然语言声明 |
| `status` | TEXT NOT NULL DEFAULT 'active' | **CHECK(status IN ('active','retracted','uncertain'))** |
| `created_at` | TEXT NOT NULL | |
| `updated_at` | TEXT NOT NULL | |
| `retraction_reason` | TEXT | 可空 |

> `status` 的 CHECK 约束在 DB 层强制 3 状态。**无 `ground_weakened`**——它是报告原因，非存储状态。

### `evidences`

| 列 | 类型 | 说明 |
|----|------|------|
| `id` | TEXT PRIMARY KEY | UUID v4 |
| `assertion_id` | TEXT NOT NULL REFERENCES assertions(id) | 归属断言 |
| `source` | TEXT NOT NULL | 如 code/test/note |
| `detail` | TEXT NOT NULL | 如 entity qualified name |
| `created_at` | TEXT NOT NULL | |

### `entity_relations`

| 列 | 类型 | 说明 |
|----|------|------|
| `id` | TEXT PRIMARY KEY | |
| `from_entity` | TEXT NOT NULL REFERENCES entities(id) | |
| `to_entity` | TEXT NOT NULL REFERENCES entities(id) | |
| `kind` | TEXT NOT NULL | contains/calls/uses |
| | UNIQUE(from_entity, to_entity, kind) | 去重 |

### `assertion_relations`

| 列 | 类型 | 说明 |
|----|------|------|
| `id` | TEXT PRIMARY KEY | |
| `from_assertion` | TEXT NOT NULL REFERENCES assertions(id) | dependent |
| `to_assertion` | TEXT NOT NULL REFERENCES assertions(id) | dependency |
| `kind` | TEXT NOT NULL | depends_on（目前仅一种） |
| | UNIQUE(from_assertion, to_assertion, kind) | |

`from → to` 语义：`from` depends_on `to`（from 成立需 to 成立）。级联沿此方向的反向边传播。

### `changelog`

| 列 | 类型 | 说明 |
|----|------|------|
| `id` | TEXT PRIMARY KEY | |
| `action` | TEXT NOT NULL | assert/retract/cascade_mark/depend/verify/sync/delete_entity |
| `target_id` | TEXT NOT NULL | |
| `detail` | TEXT NOT NULL | |
| `timestamp` | TEXT NOT NULL | |

Append-only。所有写操作追加。`cog next` 停滞检测读最近条目。

### 索引

`idx_assertions_entity`、`idx_assertions_status`、`idx_evidences_assertion`、`idx_assertion_relations_from`、`idx_assertion_relations_to`、`idx_entity_relations_from`、`idx_entity_relations_to`、`idx_changelog_target`。

PRAGMA：`foreign_keys = ON`、`journal_mode = WAL`。

## 枚举值

### EntityKind（6）

`module`、`function`、`type`、`field`、`method`、`unknown`。`infer(qname)`：大写开头→type；含 `::`→function；否则→module。

### EntityOrigin（3）

| Origin | 含义 | 晋升 |
|--------|------|------|
| `scan` | tree-sitter 自动提取 | — |
| `manual` | agent 手动（assert/depend 对不存在实体） | — |
| `experiment` | experiment commit 的 provisional 实体 | 被 sync 发现于代码 → `scan` |

### AssertionKind（5）

`contract`、`intent`、`invariant`、`fragility`、`correction`。

### AssertionStatus（3）

| 状态 | 设置 |
|------|------|
| `active` | assert 创建；recover --apply 恢复 |
| `retracted` | retract（级联源头） |
| `uncertain` | TMS 级联（cascade_mark） |

### CascadeReason（2，仅报告）

`marked_uncertain`（存储状态变 uncertain）、`ground_weakened`（仍 active，仅标记削弱）。

### EntityRelationKind（3）

`contains`（组合）、`calls`（运行时调用）、`uses`（结构依赖）。

### AssertionRelationKind（1）

`depends_on`。

### ChangelogAction（7）

`assert`、`retract`、`cascade_mark`、`depend`、`verify`、`sync`、`delete_entity`。

### Visibility（3）

`private`（默认）、`public`、`restricted`（如 Rust `pub(crate)`）。

### ExportFormat（3）

`json`、`toml`、`dot`。

## ID 约定

- 所有主键是 UUID v4 字符串。
- **展示用短 ID**：`short_id(id)` 返回前 8 字符。所有命令接受短/全 ID，经 `resolve_assertion_id`/`resolve_entity` 解析。
- 短 ID 解析：精确匹配 → 后缀模糊匹配（多义时报错给建议）。

## 命名约定

- **qualified name**：`::` 分隔（Rust 惯例），如 `cog::repo::SqliteRepository`。
- 用户输入可能是 Python 风格 `.`——`naming::normalize` 自动归一为 `::`。
- `path_to_qualified`：`src/repo/sqlite.rs` → `repo::sqlite`（剥离 src/lib/pkg 前缀 + 扩展名）。

## Grounds 格式

`source:detail`，两部分须非空。无冒号 → source="note"、detail=整体。`assert` 时 `validate_format()` 校验。常见 source：`code`、`test`、`note`、`plan`、`hypothesis`、`issue`。

## 迁移

`open()` 在加载 schema 后做加性迁移（检测列是否存在 → ADD COLUMN）：
- `origin` 列（default 'manual'）
- `metrics_json` 列

迁移只加不删——见 AGENTS.md 数据保全条款。
