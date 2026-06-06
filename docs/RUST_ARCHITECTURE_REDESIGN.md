# Cog 架构：六层认知模型

> 日期：2026-06-06
> 前置阅读：`docs/COGNITIVE_MODEL_DESIGN.md`

---

## 0. 背景：从 "Java in Rust" 到 Rust 地道设计

在对原始代码库进行审计后，识别出以下反模式——它们并非 bug，但使代码在 Rust 生态中显得"水土不服"。

### 0.1 God Object：`Store`（1200+ 行）

```rust
// 原始：一个 struct 承载了所有职责
pub struct Store { conn: Connection }

impl Store {
    // 连接管理 (3 methods)
    // CRUD: Entity (7 methods) + Assertion (8 methods) + Evidence (5 methods)
    // + Relation CRUD (8 methods) + Query methods (6 methods)
    // 40+ methods total
}
```

Java 的 `EntityManager` / `SessionFactory` 直译——一个胖 service 对象掌管一切数据访问。

### 0.2 贫血数据模型

```rust
pub struct Entity {
    pub id: String,
    pub qualified_name: String,
    pub kind: EntityKind,
    pub origin: EntityOrigin,
    pub created_at: DateTime<Utc>,
}
// Entity 自己不知道如何验证、如何展示——所有行为外化到 Store 和 format.rs
```

Martin Fowler 所谓的 "Anemic Domain Model"——Rust 社区称之为 "data bags"。

### 0.3 过程式 Command = Service Layer

```rust
pub fn execute(store: &Store, entity: &str, kind: AssertionKind, ..) -> Result<CommandOutput> {
    let entity_record = store.upsert_entity(..)?;
    let assertion = store.create_assertion(..)?;
    Changelog::append(store, ..)?;
    let mut out = String::new();
    out.push_str("assertion created\n");  // 手动拼字符串输出
    Ok(CommandOutput::success(out))
}
```

Java 的 `@Service class AssertionService { public CommandOutput execute(..) { .. } }`。

### 0.4 格式化工具类

`format.rs` 中 14 个公共自由函数，全部接受数据引用返回 String——这就是 `StringUtils` / `ReportFormatter`。

### 0.5 平面模块 + 星形依赖

```
model/                    ← 所有数据 + 所有行为，扁平堆积
  store.rs  (1200行), types.rs, graph.rs, diff.rs, branch.rs
command/                  ← 每个命令依赖 Store + format
  assert_cmd.rs → store + format
  query.rs      → store + format
  ...
```

依赖图是一个星形：`Store` 在中心，所有东西直接依赖它。

### 0.6 为什么这是问题

| 反模式 | 后果 |
|--------|------|
| God Object Store | 任何改动需要理解 1200 行上下文；无法单独测试某个数据操作 |
| 贫血数据模型 | 数据不变量散落在 Store 方法中，无法由类型系统强制执行 |
| 过程式 Service | 命令间无共享抽象；新增命令 = 复制粘贴模式 |
| 工具类 Formatter | 输出格式与领域逻辑耦合；无法切换输出模式而不改所有命令 |
| 平面模块 | 无层次，无边界，新人不知道从哪里开始读 |

---

## 1. 设计原则

五条从 Rust 社区共识中提炼的原则。

### 原则一：数据为王，行为跟随

> *"Structs hold data; impl blocks add behavior; traits define shared contracts."*

领域类型只做纯计算。副作用通过 `Repository` trait 在 command 层完成。

### 原则二：模块是封装边界，不是文件夹

好的模块暴露 3-5 个公共类型/函数，其余全部 `pub(crate)` 内部化。避免 `pub use *` 或 `pub use everything` 的模块。

### 原则三：Trait 只为真正的多态而存在

> *"Don't abstract for abstraction's sake."*

`Repository` trait 是唯一需要 trait 抽象的层——因为变更隔离和可测试性在此有真正价值。其他层（command、space、format）不需要 trait。

### 原则四：Typestate 编码进程内业务规则

> *"If a state transition is illegal, don't make it a runtime error — make it uncompilable."*

适用于进程内的 API 保证，不适用于无状态的 CLI 工作流。

### 原则五：组合优于分类

> *"Compose small, focused types; resist the urge to build deep type hierarchies."*

Rust 没有继承，组合是唯一的代码复用方式——这是优势。

---

## 2. 目标架构：六层认知模型

```
┌─────────────────────────────────────────────────────────────┐
│  第 6 层  实验层 (Experiment Layer)                         │
│  Experiment, ExperimentOp, ExperimentReport                 │
│  "如果假设 X 成立，会发生什么？"                            │
├─────────────────────────────────────────────────────────────┤
│  第 5 层  认知潜空间 (Cognitive Latent Space)               │
│  ┌──────────────────────┐ ┌──────────────────────────────┐  │
│  │ 结构子空间 (自动)    │ │ 语义子空间 (手动/Agent输入)  │  │
│  │ StructureSpace       │ │ SemanticSpace                │  │
│  │ 骨架 + 低层关系      │ │ 契约/意图/不变量/风险/修正   │  │
│  └──────────────────────┘ └──────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  第 4 层  工作流引导 (Workflow Guide)                       │
│  WorkflowState: Uninit → Ready → Changing                   │
│  "现在该做什么？" — CLI 内嵌最佳实践                       │
├───────────────────────────┬─────────────────────────────────┤
│  第 3 层  自动解析        │  第 3 层  手动操作              │
│  (Analysis Pipeline)      │  (Interactive Modeling)         │
│  Scanner, ParserPool,     │  assert, retract, depend,       │
│  FileWalker, Extractors   │  query, index, stats, verify    │
│  确定性、零 LLM           │  Agent 驱动的认知构建           │
├───────────────────────────┴─────────────────────────────────┤
│  第 2 层  持久化抽象 (Persistence)                          │
│  Repository trait, SqliteRepository                         │
├─────────────────────────────────────────────────────────────┤
│  第 1 层  代码空间 (Code Space) — 被建模的外部世界          │
│  源代码文件、目录结构、编程语言语法树                       │
└─────────────────────────────────────────────────────────────┘
```

每层只依赖直接在它下面的层。编译期强制执行。

---

## 3. 各层详细设计

### 3.1 第 1 层：代码空间 (Code Space)

不是 CLI 的一部分，但是所有操作的**根数据源**。

```
Code Space
├── 源文件 (src/**/*.rs, src/**/*.py, ...)
├── 目录结构 (模块层次)
├── 语言语法 (Rust, Python, JavaScript, Go, C, Java)
└── 其他结构化内容 (Cargo.toml, pyproject.toml, ...)
```

CLI 不修改代码空间——它只**读取**和**建模**它。

### 3.2 第 2 层：持久化抽象 (Persistence) — `repo/`

唯一需要 trait 抽象的层。

```rust
// repo/trait.rs
pub trait Repository {
    // ── Entity ──
    fn upsert_entity(&self, name: &str, kind: EntityKind, origin: EntityOrigin) -> Result<Entity>;
    fn get_entity(&self, id: &str) -> Result<Option<Entity>>;
    fn get_entity_by_name(&self, name: &str) -> Result<Option<Entity>>;
    fn list_entities(&self) -> Result<Vec<Entity>>;
    fn delete_entity(&self, id: &str) -> Result<()>;

    // ── Assertion ──
    fn create_assertion(&self, entity_id: &str, kind: AssertionKind,
                        claim: &str, grounds: &str, depends_on: Option<&str>) -> Result<Assertion>;
    fn get_assertions_for_entity(&self, entity_id: &str) -> Result<Vec<Assertion>>;
    fn update_assertion_status(&self, id: &str, status: AssertionStatus) -> Result<()>;

    // ── Evidence ──
    fn create_evidence(&self, assertion_id: &str, source: &str, detail: &str) -> Result<Evidence>;

    // ── Relations ──
    fn add_entity_relation(&self, from: &str, to: &str, kind: EntityRelationKind) -> Result<()>;
    fn get_related_entities(&self, entity_id: &str) -> Result<Vec<RelatedEntity>>;

    // ── Graph queries ──
    fn get_downstream_assertions(&self, assertion_id: &str) -> Result<Vec<Assertion>>;
    fn get_upstream_assertions(&self, assertion_id: &str) -> Result<Vec<Assertion>>;

    // ── Short ID resolution ──
    fn resolve_short_id(&self, short_id: &str) -> Result<Option<String>>;

    // ── Aggregate queries ──
    fn stats(&self) -> Result<ModelStats>;
    fn count_unasserted_entities(&self) -> Result<usize>;
    fn count_uncertain_assertions(&self) -> Result<usize>;

    // ── Transaction (where Self: Sized — 非 object-safe) ──
    fn transaction<F, T>(&self, f: F) -> Result<T> where F: FnOnce() -> Result<T>;
}
```

`SqliteRepository` 实现 `Repository`，按职责拆分为子模块：

```
repo/sqlite/
├── mod.rs          # SqliteRepository struct + open/open_in_memory
├── entities.rs     # Entity CRUD + upsert + metrics
├── assertions.rs   # Assertion CRUD + status updates + dependencies
├── evidence.rs     # Evidence CRUD
├── relations.rs    # Entity/Assertion relations
├── changelog.rs    # Changelog append
├── stats.rs        # Aggregate queries
├── helpers.rs      # Schema init, short ID resolution, migrations
├── trait_impl.rs   # Repository trait impl delegating to submodules
└── tests.rs        # Internal unit tests
```

**关键决策：**

- 测试使用 `SqliteRepository::open_in_memory()`——真实 SQLite 语义，零磁盘 I/O。不实现 `InMemoryRepository`：`HashMap` 无法忠实模拟 FOREIGN KEY、事务隔离、级联删除。
- `transaction()` 方法是泛型的（`where Self: Sized`），因此无法通过 `&dyn Repository` 调用。需要事务的命令（`retract`）接受具体类型 `&SqliteRepository`；其余 12 个命令全部使用 `&dyn Repository`。
- 不引入 ORM、连接池或复杂事务抽象。Cog 是单用户 CLI 工具，`rusqlite::Connection` 直接使用。

### 3.3 第 3 层：双通道建模

#### 通道 A：自动解析 (Analysis Pipeline) — `analysis/`

```rust
// analysis/scanner.rs
pub struct Scanner { pool: ParserPool }

impl Scanner {
    pub fn new() -> Self { .. }
    pub fn scan(&mut self, config: &ScanConfig) -> Result<ScanReport> { .. }
}

// analysis/pool.rs
pub struct ParserPool { parsers: HashMap<Language, tree_sitter::Parser> }

impl ParserPool {
    pub fn acquire(&mut self, lang: Language) -> Result<&mut tree_sitter::Parser> { .. }
}

// analysis/report.rs
pub struct ScanReport {
    pub files_scanned: usize,
    pub entities_found: usize,
    pub new_entities: Vec<Entity>,
    pub existing_entities: Vec<Entity>,
    pub new_relations: Vec<(String, String, EntityRelationKind)>,
    pub languages_detected: Vec<Language>,
}
```

```
analysis/
├── mod.rs
├── scanner.rs      # Scanner: 遍历 + 调度
├── walker.rs       # FileWalker: BFS 文件系统遍历，跳过隐藏目录
├── pool.rs         # ParserPool: 按语言缓存 tree-sitter Parser
├── report.rs       # ScanReport: 纯数据扫描结果
├── languages.rs    # Language enum (Rust, Python, JS, Go, C, Java)
└── extractors/     # 语言特定提取器（每个语言一个文件）
    ├── mod.rs
    ├── rust.rs, python.rs, javascript.rs, go.rs, c.rs, java.rs
```

**关键设计：**
- `Scanner::scan()` **不**接触 `Repository`——返回纯数据 `ScanReport`，由 `init` 命令负责写入。
- `ParserPool` 按语言缓存 tree-sitter Parser，避免每次解析文件都 `Parser::new()`。
- 确定性、零 LLM 开销、幂等。

#### 通道 B：手动操作 (Interactive Modeling) — `command/`

12 个命令，每个接受 `&dyn Repository`（retract 除外：需要 `transaction()`，接受 `&SqliteRepository`）。

```
command/
├── mod.rs
├── init_cmd.rs       # cog init — 扫描代码库，构建初始模型
├── assert_cmd.rs     # cog assert — 记录断言（contract/invariant/fragility/correction）
├── retract.rs        # cog retract — 撤销断言 + TMS 级联
├── depend.rs         # cog depend — 记录实体关系
├── query.rs          # cog query — 查询实体认知卡片
├── impact.rs         # cog impact — 下游影响面分析
├── trace.rs          # cog trace — 依赖链追溯
├── index_cmd.rs      # cog index — 实体索引列表
├── stats.rs          # cog stats — 模型统计
├── verify.rs         # cog verify — 结构一致性验证
├── export.rs         # cog export — 模型导出 (JSON/TOML/DOT)
├── experiment_cmd.rs # cog experiment — 实验管理
├── backup_cmd.rs     # cog backup — 全量备份
└── entity_cmd.rs     # cog delete-entity — 实体删除
```

**命令模式：**

```rust
// 每个命令返回 CommandOutput { text, exit_code }
pub fn execute(repo: &dyn Repository, ..., output: OutputFormat) -> Result<CommandOutput> {
    // 1. 验证输入
    // 2. 通过 Repository 执行操作
    // 3. 构建报告类型（如 QueryCard, ImpactCard）
    // 4. 通过 format::emit_report() 路由输出
    let report = QueryCard { entity, assertions, related };
    Ok(CommandOutput::success(format::emit_report(&report, output)))
}
```

与 Java Service 的本质区别：

| | Java Service (原) | 领域命令 (目标) |
|---|---|---|
| **依赖方向** | 命令直接 import Store 具体方法 | 命令只依赖 `Repository` trait |
| **输出格式** | 命令内拼 String | 返回领域类型，格式化在外层 |
| **职责** | 验证 + 存储 + 格式化 + changelog 全做 | 只做验证 + 存储，单一职责 |

### 3.4 第 4 层：工作流引导 (Workflow Guide) — `workflow/`

核心洞察：CLI 接口太底层——每个命令是原子操作，Agent 需要自己知道调用顺序。

**解决：CLI 内嵌最佳实践。** 每个命令执行时自动读取 `.cog/workflow_state.json`，根据状态转换规则更新状态。唯一新增的是 `cog next`——查看当前状态和下一步建议。

```rust
// workflow/state.rs
pub enum WorkflowState {
    Uninit,
    Ready { phase: WorkflowPhase },
    Changing { description: String, started_at: DateTime<Utc>, affected_entities: Vec<String> },
}

pub enum WorkflowPhase {
    FreshScan,    // 刚 init，还没有 assertion
    Exploring,    // 浏览、查询、记录断言
    Assessing,    // 正在运行 impact/trace 评估
    PostChange,   // verify 通过，等待记录修正
    Debugging,    // retract 触发 TMS 级联，或 verify 发现不一致
}
```

**完整状态转换规则：**

```
1. 顶级状态转换
Uninit --init--> Ready { FreshScan }
Ready { * } / Changing --init--> 错误: "already initialized"
Ready { * } --start-change--> Changing { .. }
Changing --finish-change--> Ready { Exploring }
Changing --abort-change--> Ready { Exploring }

2. Ready 状态内的 Phase 转换
FreshScan --query/assert (首次)--> Exploring
FreshScan --impact/trace--> Assessing
FreshScan --retract--> Debugging
Exploring --impact/trace--> Assessing
Exploring --retract--> Debugging
Assessing --query/assert/depend--> Exploring
Assessing --retract--> Debugging
PostChange --query/assert/depend--> Exploring
PostChange --impact/trace--> Assessing
PostChange --retract--> Debugging
Debugging --verify (--clean 通过)--> Exploring
Debugging --verify (未通过)--> Debugging (不变)

3. Changing 状态内的转换
Changing --verify (pass)--> Ready { PostChange }
Changing --verify (fail)--> Changing (保持)
Changing --assert/query/retract--> Changing (不变)

4. 无状态影响的命令
depend, delete-entity, index, stats, export, experiment --> 不改变顶级状态
```

**建议引擎** (`workflow/suggestions.rs`)：根据当前状态 + 模型数据，返回 `Vec<SuggestedAction>`。Agent 调用 `cog next` 即可知道下一步。

```rust
pub struct SuggestedAction {
    pub action: ActionKind,
    pub description: String,      // "32 entities have no assertions yet."
    pub why: String,              // "Core modules need contracts before any change."
    pub example_command: String,  // "cog assert <entity> --kind contract --claim \"...\""
}
```

**为什么不用 Typestate？** CLI 每次调用是新进程，typestate 的编译期保证无法跨进程传递。CLI 层用序列化的 `WorkflowState` enum + 建议引擎；typestate 保留给进程内对象（如 `Experiment`）。

### 3.5 第 5 层：认知潜空间 (Cognitive Latent Space) — `space/`

两个子空间——共享 entity 作为锚点，但数据类型和操作截然不同。

#### 结构子空间 (`space/structure.rs`)

```rust
pub struct StructureSpace {
    entities: HashMap<String, EntityNode>,
    edges: Vec<StructureEdge>,
}

impl StructureSpace {
    pub fn load(repo: &dyn Repository, root: &str, depth: usize) -> Result<Self> { .. }
    pub fn load_adaptive(repo: &dyn Repository, root: &str, max_nodes: usize)
        -> Result<(Self, Vec<String>)> { .. }  // (space, boundary_entities)
    pub fn dependents_of(&self, entity: &str) -> Vec<&EntityNode> { .. }
    pub fn dependencies_of(&self, entity: &str) -> Vec<&EntityNode> { .. }
}
```

从 Repository 加载所需子图（BFS），纯内存操作，不持有数据库连接。

#### 语义子空间 (`space/semantic.rs`)

```rust
pub struct SemanticSpace {
    assertions: HashMap<String, AssertionNode>,
    evidence: HashMap<String, EvidenceNode>,
    depends_on: Vec<(String, String)>,
}

impl SemanticSpace {
    pub fn load(repo: &dyn Repository, entity_id: &str) -> Result<Self> { .. }
    pub fn simulate_retract(&self, assertion_id: &str) -> CascadeReport { .. }
    pub fn trace(&self, assertion_id: &str) -> TraceTree { .. }
    pub fn assess_risk(&self, entity: &str, structure: &StructureSpace) -> RiskAssessment { .. }
}
```

**关键设计：模拟与执行分离**

| 操作 | 实现位置 | 说明 |
|------|---------|------|
| `SemanticSpace::simulate_retract` | `space/semantic.rs` | 纯内存模拟——用于 Experiment 评估和 impact 分析 |
| `CascadeEngine::retract` | `space/cascade.rs` | 两阶段：先加载 SemanticSpace 验证上下文，再通过 Repository 执行真实 BFS 级联 |
| `ImpactEngine::analyze` | `space/impact.rs` | BFS 影响面分析，集成 `assess_risk` |
| `TraceEngine::analyze` | `space/trace.rs` | DFS 依赖链追溯 |

CascadeEngine 保留两阶段模式：因为"是否有独立活跃依赖"的检查需要实时查询 Repository 状态（局部子图不完整），无法在纯内存中可靠完成。

#### 风险评估 (`space/risk.rs`)

```rust
pub struct RiskAssessment {
    pub entity_name: String,
    pub risk_score: f64,           // 0.0 (安全) → 1.0 (高风险)
    pub downstream_count: usize,
    pub active_assertions: usize,
    pub fragile_assertions: usize,
    pub summary: String,
}
```

通过 `ImpactEngine::analyze()` 自动计算，结果嵌入到 `ImpactCard.risk_assessment` 字段中。

### 3.6 第 6 层：实验层 (Experiment Layer) — `experiment/`

> *"在潜空间中推演，而不是在代码空间中试错"*

Experiment 是**单根假设推理工具**——围绕一个不确定的变更点，模拟它的传播后果。

```
experiment/
├── mod.rs
├── session.rs      # Experiment 主类型
├── ops.rs          # ExperimentOp enum
├── report.rs       # ExperimentReport, Contradiction
└── persistence.rs  # 序列化/恢复 (跨 session)
```

```rust
// experiment/session.rs
pub struct Experiment {
    pub id: String,
    pub description: String,
    pub saved: bool,           // draft (unsaved) vs checkpoint (saved)
    pub status: ExperimentStatus,
    pub structure: StructureSpace,
    pub semantic: SemanticSpace,
    pub staged: Vec<ExperimentOp>,
    pub boundary_entities: Vec<String>,
}

pub enum ExperimentStatus { Open, Evaluated, Committed, Discarded }
```

**ExperimentOp：**
```rust
pub enum ExperimentOp {
    Assertion { entity: String, kind: AssertionKind, claim: String, grounds: String },
    Retraction { assertion_id: String, reason: String },
    Relation { from: String, to: String, kind: EntityRelationKind },
    DeleteEntity { entity: String },
}
```

**设计要点：**

1. **轻量级快照**：`start()` 通过 BFS 加载依赖子图到内存（自适应深度，默认最大 500 节点），不复制整个 DB。
2. **跨 session 持久化**：序列化到 `.cog/experiments/<id>.json`，不依赖 `VACUUM INTO`。
3. **draft/saved 语义**：实验创建时 `saved: false`（draft），`save` 命令标记为 checkpoint。`list` 区分 draft/saved。
4. **commit 是 replay**：`commit()` 将 staged 操作在真实 DB 上回放——确定性操作日志回放，不需要 UUID 冲突解决，比 diff-then-merge 更可靠。
5. **Experiment 与 workflow 并行**：不影响顶级 `WorkflowState`，在建议引擎中按场景推荐。

#### Branch 的降级定位 — `backup/`

原 Branch 设计（`VACUUM INTO` 复制整个 SQLite → diff/merge）存在 merge 语义薄弱、UUID 冲突、开销不对称等问题。Branch 降级为全量模型备份工具：

```bash
cog backup create --name "before-major-refactor"   # VACUUM INTO
cog backup list
cog backup restore "before-major-refactor"
cog backup drop "before-major-refactor"
```

日常假设推理走 Experiment；大规模架构变更前用 Backup 做安全网。

### 3.7 横切关注点：输出格式化 — `format/`

```rust
// format/mod.rs
pub enum OutputFormat { Text, Json }

pub trait Renderable {
    fn render_text(&self) -> String;
}

pub fn emit_report<T: serde::Serialize + Renderable>(report: &T, format: OutputFormat) -> String {
    match format {
        OutputFormat::Text => report.render_text(),
        OutputFormat::Json => json::JsonRender::render(report),
    }
}
```

```
format/
├── mod.rs      # OutputFormat, Renderable trait, emit_report(), 向后兼容 free functions
├── text.rs     # TextRenderer — 人类可读文本输出
└── json.rs     # JsonRender — JSON 序列化
```

**报告类型**（定义在 `domain/report.rs`）同时 derive `Serialize` 和实现 `Renderable`：

| 报告类型 | 用途 | 命令 |
|---------|------|------|
| `QueryCard` | 实体查询结果 | `cog query` |
| `EntityIndex` | 实体索引列表 | `cog index` |
| `ImpactCard` | 影响面分析（含 risk_assessment） | `cog impact` |
| `TraceTree` | 依赖链追溯 | `cog trace` |
| `CascadeReport` | 撤销级联结果 | `cog retract` |
| `InitReport` | 扫描初始化结果 | `cog init` |
| `VerificationReport` | 一致性验证结果 | `cog verify` |
| `ModelStats` | 模型统计 | `cog stats` |
| `StatusMessage` | 简单状态消息 | `cog assert/depend` |

---

## 4. 模块结构

```
src/
├── main.rs                    # 入口点：装配依赖，分发命令
├── cli/                       # Clap CLI 定义
│   ├── mod.rs                 # Cli struct + Commands enum
│   ├── args.rs                # 每个命令的参数 struct
│   ├── experiment.rs          # ExperimentAction enum
│   └── backup.rs              # BackupAction enum
├── domain/                    # ── 领域核心类型 ──
│   ├── mod.rs
│   ├── entity.rs              # Entity, EntityKind, EntityOrigin, last_segment(), parent_qname()
│   ├── assertion.rs           # Assertion, AssertionKind, AssertionStatus
│   ├── evidence.rs            # Evidence
│   ├── relations.rs           # EntityRelation, AssertionRelation, relation kinds, RelatedEntity
│   ├── changelog.rs           # ChangelogEntry, ChangelogAction
│   ├── metrics.rs             # EntityMetrics (fan_in, fan_out, line_count, visibility)
│   ├── grounds.rs             # Grounds: source:detail 格式 + validate_format()
│   └── report.rs              # 所有命令报告类型 + ModelStats, ExportFormat, ModelSnapshot
├── repo/                      # ── 持久化层 ──
│   ├── mod.rs
│   ├── trait.rs               # Repository trait
│   └── sqlite/                # SqliteRepository（拆分为 10 个子模块）
│       ├── mod.rs, entities.rs, assertions.rs, evidence.rs
│       ├── relations.rs, changelog.rs, stats.rs, helpers.rs
│       ├── trait_impl.rs, tests.rs
├── analysis/                  # ── 自动解析管道 ──
│   ├── mod.rs
│   ├── scanner.rs             # Scanner: 遍历 + 调度
│   ├── walker.rs              # FileWalker: BFS 文件系统遍历
│   ├── pool.rs                # ParserPool: 按语言缓存 tree-sitter Parser
│   ├── report.rs              # ScanReport: 纯数据扫描结果
│   ├── languages.rs           # Language enum
│   └── extractors/            # 语言特定提取器
│       ├── mod.rs, rust.rs, python.rs, javascript.rs, go.rs, c.rs, java.rs
├── command/                   # ── 手动操作（12 个命令）──
│   ├── mod.rs
│   ├── init_cmd.rs, assert_cmd.rs, retract.rs, depend.rs
│   ├── query.rs, impact.rs, trace.rs, index_cmd.rs
│   ├── stats.rs, verify.rs, export.rs, entity_cmd.rs
│   ├── experiment_cmd.rs, backup_cmd.rs
├── space/                     # ── 认知潜空间 ──
│   ├── mod.rs
│   ├── structure.rs           # StructureSpace (结构子空间)
│   ├── semantic.rs            # SemanticSpace (语义子空间，含 simulate_retract/trace/assess_risk)
│   ├── cascade.rs             # CascadeEngine (两阶段：SemanticSpace 验证 + Repository 级联)
│   ├── impact.rs              # ImpactEngine (BFS 影响面 + risk 集成)
│   ├── trace.rs               # TraceEngine (DFS 依赖链)
│   └── risk.rs                # RiskAssessment
├── experiment/                # ── 实验层 ──
│   ├── mod.rs
│   ├── session.rs             # Experiment 主类型
│   ├── ops.rs                 # ExperimentOp
│   ├── report.rs              # ExperimentReport
│   └── persistence.rs         # 序列化/恢复
├── workflow/                  # ── 工作流状态机 ──
│   ├── mod.rs
│   ├── state.rs               # WorkflowState enum
│   └── suggestions.rs         # SuggestedAction, ActionKind, 建议引擎
├── backup/                    # ── 全量模型备份 ──
│   ├── mod.rs
│   └── manager.rs             # BackupManager: create/list/restore/drop
└── format/                    # ── 输出格式化 ──
    ├── mod.rs                 # OutputFormat, Renderable trait, emit_report()
    ├── text.rs                # TextRenderer
    └── json.rs                # JsonRender
```

### 4.1 依赖方向

```
main ──→ cli ──→ command ──→ space ──→ repo ──→ domain
        │         │
        │         ├──→ workflow (状态文件读写)
        │         ├──→ analysis ──→ repo ──→ domain
        │         ├──→ experiment ──→ space ──→ repo ──→ domain
        │         └──→ backup ──→ repo ──→ domain
        │
        └──→ format
```

`cli` 编排 `command`、`workflow`、`analysis`、`experiment`、`backup`。各子系统之间不互相依赖。

---

## 5. 关键类型设计

### 5.1 Entity

```rust
pub struct Entity {
    pub id: String,                // UUID v4
    pub qualified_name: String,    // "::" 分隔的路径，如 "cog::repo::sqlite::SqliteRepository"
    pub kind: EntityKind,          // Function, Type, Module (由 EntityKind::infer 推断)
    pub origin: EntityOrigin,      // Scan (自动) / Manual (手动)
    pub metrics: EntityMetrics,    // fan_in, fan_out, line_count, visibility
    pub created_at: DateTime<Utc>,
}
```

**自由函数（替代 newtype）：**

```rust
// domain/entity.rs
pub fn last_segment(qname: &str) -> &str { .. }          // "a::b::C" → "C"
pub fn parent_qname(qname: &str) -> Option<&str> { .. }   // "a::b::C" → Some("a::b")
```

Newtype 方案（`QualifiedName`/`EntityId`/`AssertionId`）经评估后放弃——Rust trait-object + coherence 限制下迁移成本过高（需改 `HashSet`/`HashMap`/`Repository` trait 全量签名），收益不足以覆盖成本。

### 5.2 Grounds

```rust
pub struct Grounds {
    pub source: String,   // "code", "plan", "hypothesis", "meta-loop"
    pub detail: String,   // 具体说明
}

impl Grounds {
    pub fn parse(raw: &str) -> Self { .. }              // "code:my::fn" → { source: "code", detail: "my::fn" }
    pub fn validate_format(&self) -> anyhow::Result<()> { .. }  // 确保 source 和 detail 非空
}
```

### 5.3 EntityMetrics

```rust
pub struct EntityMetrics {
    pub fan_in: Option<u32>,
    pub fan_out: Option<u32>,
    pub line_count: Option<u32>,
    pub visibility: Visibility,
}
```

`fan_in`/`fan_out` 在 `init` 命令中通过扫描实体关系自动计算。

---

## 6. 设计决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| Repository 用 trait 还是具体类型？ | trait | 唯一真正需要多态的地方。其他层不需要 trait。 |
| 测试用 InMemory 还是 SQLite `:memory:`？ | SQLite `:memory:` | HashMap 无法忠实模拟 FK 约束、事务隔离、级联删除。`:memory:` 保留 100% SQL 保真度。 |
| sqlite.rs 拆分策略？ | 10 个子模块，按领域概念拆分 | entities/assertions/evidence/relations/changelog/stats/helpers/trait_impl/tests + mod |
| 图算法用纯函数还是方法？ | 方法（在 Space 类型上） | `space.simulate_retract(id)` 比自由函数更清晰地表达操作语义。 |
| 撤销的真实执行 vs 模拟？ | 分离 | `CascadeEngine::retract`（真实，两阶段）+ `SemanticSpace::simulate_retract`（纯内存）。Experiment 用模拟，CLI `retract` 执行真实操作。 |
| 命令放 command/ 还是 ops/？ | command/ | CLI 命令和领域命令在当前规模下合一更简洁。 |
| 状态管理如何暴露？ | 现有命令内嵌状态管理 | 每个命令自动读取/更新 `.cog/workflow_state.json`。新增仅 `cog next`/`start-change`/`finish-change`/`abort-change`。 |
| CLI 工作流用不用 typestate？ | 序列化 enum | CLI 每次调用是新进程，typestate 的编译期保证无法跨进程传递。 |
| Experiment 用 typestate？ | 运行时状态检查 | Experiment 必须支持跨 session 序列化，typestate 的编译期标记无法与 JSON 反序列化共存。运行时检查提供等效安全保证。 |
| Branch 与 Experiment 的关系？ | Experiment 单根推理 + Backup 全量备份 | Branch 降级为 backup。Merge 语义薄弱，UUID 共享导致合并歧义。 |
| Experiment commit 用 diff-merge 还是 replay？ | replay 操作日志 | 确定性回放，不需要 UUID 冲突解决。比 diff-then-merge 更简单可靠。 |
| Experiment draft/save 语义？ | 自动持久化 draft + save 标记 checkpoint | 避免丢失未保存的工作；`list` 区分 draft/saved。 |
| 格式化用 Display 还是 Serialize + Renderer？ | `Serialize` + `Renderable` trait + `TextRenderer` | `Display` 锁死单一输出格式。`Renderable` + 独立 `TextRenderer` 使加 `--output json` 是加法。 |
| 错误处理用 anyhow 还是自定义？ | `anyhow` | CLI 工具不需要细粒度错误匹配。 |
| WorkflowPhase 有几个？ | 5 个：FreshScan, Exploring, Assessing, PostChange, Debugging | Exploring 涵盖一切模型交互，去掉 Modeling（与 Exploring 语义重叠）。 |
| `init` 可否重复调用？ | 仅在 Uninit 下可用 | Ready/Changing 状态下 `init` 报错 "already initialized"，防止误操作重置模型。 |
| `retract` 后如何退出 Debugging？ | 只有 `verify --clean` 通过才退出 | 浏览操作（index/stats/export）不退出 Debugging——浏览不等于修好了。 |
| `start-change` 接受 `--entity`？ | 不接受 | `affected_entities` 应由 verify 动态检测，不由 agent 手动指定。 |
| Newtype (QualifiedName/EntityId)？ | 不采用 | Rust trait-object + coherence 限制下迁移成本过高。改用 last_segment()/parent_qname() 自由函数。 |

---

## 7. 与 COGNITIVE_MODEL_DESIGN.md 的关系

本文档是 `docs/COGNITIVE_MODEL_DESIGN.md` 的**架构实现层对应**：

| COGNITIVE_MODEL_DESIGN.md | 本文档 |
|---------------------------|--------|
| 理论与数学基础（TMS, 潜空间, 级联算法） | Rust 地道的工程实现 |
| "是什么" 和 "为什么" | "怎么做" 和 "怎么做对" |
| Entity/Assertion/Evidence 的语义定义 | Entity/Assertion/Evidence 的 Rust 类型设计 |
| 潜空间的四个层次 | StructureSpace + SemanticSpace 的具体结构 |
| 分支作为推理工具 | Experiment 单根推理 + Backup 全量备份 |
| 未涉及 | 工作流状态机（5 Phase 覆盖全部命令）+ 建议引擎 |

---

## 附录 A：参考文献

- [Rust Design Patterns](https://rust-unofficial.github.io/patterns/) — 官方社区编目的 Rust 模式、惯用法和反模式
- [Rust Is Beyond Object-Oriented](https://www.thecodedmessage.com/posts/oop-1-encapsulation/) — The Coded Message 系列
- [Zero To Production In Rust](https://www.zero2prod.com/) — Luca Palmieri 的 Rust 后端开发书籍
- [entrait](https://docs.rs/entrait/latest/entrait/) — Rust 依赖注入 crate
