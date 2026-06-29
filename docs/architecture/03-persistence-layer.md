# 第 3 层（持久化）：Repository trait + SqliteRepository — `src/repo/`

> 持久化抽象。`Repository` trait 是全系统**唯一**需要 trait 抽象的层——为了变更隔离（experiment）与可测试性（内存 SQLite）。其余所有命令通过 `&dyn Repository` 访问数据。

## 结构

```
src/repo/
├── trait.rs        # Repository trait（持久化契约）
└── sqlite.rs + sqlite/
    ├── (sqlite.rs) # SqliteRepository struct + open/transaction + Repository impl
    ├── entities.rs     # Entity CRUD + upsert + resolve + metrics
    ├── assertions.rs   # Assertion CRUD + status + resolve_assertion_id
    ├── evidence.rs     # Evidence CRUD
    ├── relations.rs    # entity/assertion relations + 依赖图查询
    ├── changelog.rs    # changelog append + list
    ├── stats.rs        # 聚合统计查询
    ├── helpers.rs      # SCHEMA 常量 + 行映射 + 时间戳 + 迁移
    └── tests.rs        # 内部单元测试
```

`sqlite.rs` 既是模块声明文件，也包含 `SqliteRepository` struct 定义、`open()`/`transaction()`/`checkpoint_wal()`，以及 `impl Repository for SqliteRepository`（委托给子模块函数）。

## Repository trait

`src/repo/trait.rs` 定义契约，按职责分组（约 35 个方法）：

- **Entity**：`upsert_entity`、`get_entity`、`get_entity_by_name`、`resolve_entity`（模糊后缀匹配）、`list_entities`、`list_entities_filtered`、`delete_entity`、`ensure_manual_entity`（默认方法）。
- **Assertion**：`create_assertion`、`get_assertion`、`get_assertions_for_entity(s)`、`list_assertions`、`update_assertion_status`、`retract_assertion`、`resolve_assertion_id`。
- **Evidence**：`get_evidence_for_assertion(s)`、`list_evidences`。
- **Relations**：`add_entity_relation`、`list_entity_relations`、`list_assertion_relations`、`get_assertion_relations_for`、`get_dependents`、`get_dependencies`、`get_related_entities`。
- **Scanning**：`get_scanned_entity_names`、`get_experiment_entity_names`（驱动 sync 的 origin 晋升）。
- **Changelog**：`append_changelog`、`list_changelog_entries`。
- **Metrics**：`update_entity_metrics`。
- **Utility**：`count_relations_for_entity`、`stats`、`vacuum_into`。

> **`transaction()` 不在 trait 上**。它是 `SqliteRepository` 的固有方法（泛型 `F: FnOnce() -> Result<T>`，object-unsafe）。需要事务的命令直接持有具体类型 `&SqliteRepository`。

## SqliteRepository

```rust
pub struct SqliteRepository { pub(crate) conn: Connection }
```

**`open(path)`**：
1. 打开连接，执行 `PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;`。
2. 执行 `SCHEMA`（`CREATE TABLE IF NOT EXISTS`，幂等）。
3. 运行**加性迁移**：检测 `origin` 列、`metrics_json` 列是否存在，不存在则 `ALTER TABLE ... ADD COLUMN`（见 [decisions/](../decisions/README.md) 的数据保全原则）。

**`open_in_memory()`**（仅 `#[cfg(test)]`）：零磁盘 I/O，完整 SQL 语义（FK 约束、事务、级联删除）。所有单元测试用此。**不**实现 `InMemoryRepository`——`HashMap` 无法忠实模拟这些 SQL 语义。

**`transaction(f)`**：`BEGIN IMMEDIATE` → `f()` → `COMMIT`/`ROLLBACK`。需要单命令原子性的内部操作（如 `delete_entity`、`transfer_entity`）使用它；`sync` 不再整体包裹，以避免与 `delete_entity` 的自身事务嵌套。

**`checkpoint_wal()`**：`PRAGMA wal_checkpoint(TRUNCATE)`，在 `backup restore` 等外部文件操作前调用，确保 WAL 写入落盘。

## Schema

6 张表 + 8 个索引（`src/repo/sqlite/helpers.rs::SCHEMA`）。完整定义见 [reference/02-data-model.md](../reference/02-data-model.md)。要点：

- 主键全为 UUID 字符串（`TEXT PRIMARY KEY`）。
- `assertions.status` 有 `CHECK(status IN ('active','retracted','uncertain'))`——数据库层面强制 3 状态。
- 外键级联：`assertions`→`entities`、`evidences`→`assertions`、`*_relations`→对应表。
- `entity_relations` 与 `assertion_relations` 都有 `UNIQUE(from, to, kind)` 去重。

## 哪些命令需要具体类型

几乎所有命令接受 `&dyn Repository` 并通过 trait 方法操作。**`sync` 仍由 `cli.rs` 传 `&SqliteRepository`**（历史原因：曾需要具体类型做外层事务），但实现上已不再依赖事务包裹，未来可改为 `&dyn Repository`。

> 历史记录：`retract` 曾因 `transaction()` 需要 `&SqliteRepository`。当前 `CascadeEngine::retract` 已重构为接受 `&dyn Repository`（级联逐条提交而非整体事务），故 `retract` 命令现在也用 `&dyn Repository`。

## 设计约束

- 迁移只允许加性变更（`ADD COLUMN`/`CREATE TABLE`）——禁止 `DROP`。见 AGENTS.md 数据保全条款。
- 不引入 ORM、连接池或复杂事务抽象——单用户 CLI，`rusqlite::Connection` 直接使用。
- 行映射集中在 `helpers.rs`（`map_entity_row`/`map_assertion_row`/`map_evidence_row`），避免散落重复。
