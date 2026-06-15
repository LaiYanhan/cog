# 第 2 层：领域类型 — `src/domain/`

> 领域核心类型。纯数据 + 纯计算，零副作用，零数据库依赖。所有其它层通过这些类型交流。修改本层意味着修改整个系统的共享词汇表——影响面最大，必须最谨慎。

## 模块结构

```
src/domain/
├── entity.rs       # Entity, EntityKind(6), EntityOrigin(3)
├── assertion.rs    # Assertion, AssertionKind(5), AssertionStatus(3), short_id()
├── evidence.rs     # Evidence
├── relations.rs    # EntityRelationKind(3), AssertionRelationKind(1), RelatedEntity
├── grounds.rs      # Grounds: source:detail 格式
├── metrics.rs      # EntityMetrics, Visibility
├── naming.rs       # qualified name 工具函数（统一 :: 处理）
├── changelog.rs    # ChangelogAction(7), ChangelogEntry
├── report.rs       # 所有命令报告类型 + ModelStats/ExportFormat/ModelSnapshot
├── display.rs      # 跨命令共享的展示辅助
└── risk.rs         # RiskAssessment
```

`src/domain.rs` 是模块声明文件，re-export 所有公共类型供其它层 `use crate::domain::*`。

## 核心类型

### Entity

```rust
pub struct Entity {
    pub id: String,                // UUID v4
    pub qualified_name: String,    // "::" 分隔，如 "cog::repo::SqliteRepository"
    pub kind: EntityKind,
    pub origin: EntityOrigin,
    pub metrics: EntityMetrics,
    pub created_at: DateTime<Utc>,
}
```

**`EntityKind`（6 种）**：`Module`、`Function`、`Type`、`Field`、`Method`、`Unknown`。推断启发式 `EntityKind::infer(name)`：大写开头 → `Type`；含 `::` → `Function`；否则 → `Module`。

**`EntityOrigin`（3 种）**：

| Origin | 含义 | 创建途径 |
|--------|------|---------|
| `Scan` | tree-sitter 自动提取 | `cog sync` |
| `Manual` | agent 手动声明 | `cog assert`/`depend` 对不存在的 entity |
| `Experiment` | experiment commit 创建的 provisional entity | `experiment commit`（未扫到的实体） |

`Experiment` origin 的实体在被 `cog sync` 发现于代码后晋升为 `Scan`——这是模型与代码对齐的机制。

### Assertion

```rust
pub struct Assertion {
    pub id: String,
    pub entity_id: String,
    pub kind: AssertionKind,         // Contract/Intent/Invariant/Fragility/Correction
    pub claim: String,
    pub status: AssertionStatus,     // Active/Retracted/Uncertain（仅 3 种）
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub retraction_reason: Option<String>,
}
```

**`AssertionStatus` 只有 3 种**（重要——曾被误记为 4 种）：

| 状态 | 含义 | 设置者 |
|------|------|--------|
| `Active` | 当前有效 | `assert` 创建；`recover --apply` 恢复 |
| `Retracted` | 被显式撤回 | `retract`（级联源头） |
| `Uncertain` | 依赖断裂，存疑 | TMS 级联（`cascade_mark`） |

> **`GroundWeakened` 不是存储状态**。它只是级联报告的 `CascadeReason`——表示该断言仍有其它 Active 依赖支撑，因此**保持 `Active` 不变**。见 [reference/03-tms-cascade.md](../reference/03-tms-cascade.md)。

`short_id(id)` 返回 UUID 前 8 字符，用于所有展示输出。

### Evidence

```rust
pub struct Evidence { id, assertion_id, source, detail, created_at }
```

通过 `assertion_id` 外键归属 assertion。`source`/`detail` 由 `Grounds` 解析而来。

### 关系

**实体关系 `EntityRelationKind`（3 种）**：`Contains`（组合）、`Calls`（运行时调用）、`Uses`（结构依赖）。`cog impact` 的 BFS 只跟随 `Calls`+`Uses` 的反向边——`Contains` 是结构性的，不算依赖。

**断言关系 `AssertionRelationKind`（1 种）**：`DependsOn`。这是 TMS 级联传播所沿的边。

`RelatedEntity` 携带 `RelationDirection`（Outgoing/Incoming），供 `query`/`impact` 展示。

### Grounds

`Grounds::parse("code:auth::login")` → `{ source: "code", detail: "auth::login" }`。格式 `source:detail`，两部分须非空；无冒号时整体作为 detail、source 默认 `"note"`。`assert` 时调用 `validate_format()`。

### EntityMetrics

```rust
pub struct EntityMetrics {
    pub line_count: Option<u32>,   // tree-sitter node 行范围
    pub fan_in: Option<u32>,       // 多少实体依赖它（扫描后计算）
    pub fan_out: Option<u32>,      // 它依赖多少实体
    pub visibility: Visibility,    // Private/Public/Restricted
}
```

`fan_in`/`fan_out` 在 `cog sync` 结束时由 `compute_fan_metrics()` 遍历所有 entity_relations 一次性计算并持久化。`visibility.is_public()` 用于风险评估与展示过滤。

### naming.rs

集中所有 qualified name 操作，使分隔符逻辑只存在于一处：

| 函数 | 作用 |
|------|------|
| `last_segment(qname)` | `"a::b::C"` → `"C"` |
| `parent_qname(qname)` | `"a::b::C"` → `Some("a::b")` |
| `ancestors(qname)` | `"a::b::c"` → `["a", "a::b"]` |
| `normalize(qname)` | `"pkg.mod.fn"` → `"pkg::mod::fn"`（Python 风格归一） |
| `path_to_qualified(rel)` | `"src/repo/sqlite.rs"` → `"repo::sqlite"` |

`SEP = "::"` 是规范分隔符。

### ChangelogAction（7 种）

`Assert`、`Retract`、`CascadeMark`、`Depend`、`Verify`、`Sync`、`DeleteEntity`。所有写操作追加 changelog 条目；`cog next` 的停滞检测依赖最近 changelog 序列。

## report.rs

所有命令的输出报告类型集中于此（约 20 个），如 `QueryCard`、`ImpactCard`、`SyncReport`、`NextReport`、`CascadeReport` 等。它们同时 derive `serde::Serialize` 并实现 `Renderable`，经 `format::emit_report()` 路由到 text/json 输出——见 [08-output-layer.md](08-output-layer.md)。

## 设计约束

- 本层**不依赖** `repo`、`space`、`command` 等任何上层——它是依赖图的叶子。
- 类型尽量纯数据 + 少量派生方法（如 `Entity::from_scan`、`is_active()`），不做 I/O。
- 修改枚举变体（如新增 `EntityKind`）需同步更新 `EntityKind::infer`、CLI `ValueEnum`、格式化层与持久化层的映射——这是高风险变更。
