# Cog 架构重设计：从 "Java in Rust" 到 Rust 地道设计

> 日期：2026-06-05
> 状态：核心架构已实施 (2026-06-05)
> 前置阅读：`docs/COGNITIVE_MODEL_DESIGN.md`

---

## 实施状态 (2026-06-05)

| 层次 | 状态 | 备注 |
|------|------|------|
| 第 2 层 — 持久化 (Repository trait + SqliteRepository) | ✅ 已完成 | repo/trait.rs + repo/sqlite.rs |
| 第 3 层 — 自动解析 (analysis/) | ✅ 已完成 | 无变化，import 路径已更新 |
| 第 3 层 — 手动操作 (command/) | ✅ 已完成 | 12 个命令接受 `&dyn Repository`；retract/branch 保留 `&SqliteRepository` |
| 第 4 层 — 工作流引导 (workflow/) | ✅ 已完成 | WorkflowState + suggestion engine + `cog next` 命令 |
| 第 5 层 — 认知潜空间 (space/) | ✅ 已完成 | CascadeEngine, ImpactEngine, TraceEngine |
| 第 6 层 — 实验层 (experiment/) | ✅ 已完成 | experiment/ 模块含 session/ops/report/persistence |
| 备份 (backup/) | ✅ 已完成 | backup/ 模块，Branch 标记为 deprecated |
| EntityMetrics | ✅ 已完成 | domain/metrics.rs，fan_in/fan_out 在 init_cmd 中计算 |
| QualifiedName / EntityId / AssertionId newtype | ❌ 已移除 | 改用 last_segment()/parent_qname() 自由函数；ID newtype 成本过高，放弃

### 与设计的偏差

1. **commands/ 代替 ops/**：命令函数直接写在 `command/` 中而非 `ops/` 模块，因为 CLI 命令和领域命令在当前规模下合一更简洁。
2. **&dyn Repository 而非完全 trait-only**：`retract`、`branch`、`CascadeEngine` 需要 `transaction()`（带泛型参数，非 object-safe），因此保留 `&SqliteRepository` 作为具体类型。其他 12 个命令全部使用 `&dyn Repository`。
3. **format/ 无 Renderable trait**：`Renderable` trait 无其他实现（当前仅 TextRenderer），按 YAGNI 移除。`TextRenderer` 直接以自由函数暴露。
4. **旧 model/ 完全移除**：`model/` 目录（store.rs, graph.rs, types.rs, changelog.rs）已删除。`branch.rs` 和 `diff.rs` 迁入 `repo/`。

5. **Newtypes 已移除**：`EntityId`/`AssertionId`/`QualifiedName` 的 newtype 方案因 Rust trait-object + coherence 限制，迁移成本（46+ 编译错误，需改 `HashSet`/`HashMap`/Repository trait 全量签名）远超收益。`domain/ids.rs` 已删除，改用 `entity.rs` 中的两个自由函数 `last_segment(&str) -> &str` 和 `parent_qname(&str) -> Option<&str>` 替代 `QualifiedName`。ID 字段保持 `String` 不变。

---

## 0. 诊断：当前架构的 "Java in Rust" 模式

在对当前代码库进行审计后，识别出以下反模式——它们并非 bug，但使代码在 Rust 生态中显得"水土不服"：

### 0.1 God Object：`Store`（1200+ 行）

```rust
// 当前：一个 struct 承载了所有职责
pub struct Store { conn: Connection }

impl Store {
    // 连接管理
    pub fn open(path: &Path) -> Result<Self> { .. }
    pub fn transaction<F, T>(&self, f: F) -> Result<T> { .. }
    pub fn vacuum_into(&self, target_path: &Path) -> Result<()> { .. }

    // CRUD: Entity
    pub fn upsert_entity(..) -> Result<Entity> { .. }
    pub fn insert_entity(..) -> Result<bool> { .. }
    pub fn get_entity(..) -> Result<Option<Entity>> { .. }
    pub fn get_entity_by_name(..) -> Result<Option<Entity>> { .. }
    pub fn list_entities(..) -> Result<Vec<Entity>> { .. }
    pub fn delete_entity(..) -> Result<()> { .. }
    // .. + Assertion CRUD (8 methods) + Evidence CRUD (5 methods)
    // .. + Relation CRUD (8 methods) + Query methods (6 methods)
    // .. + 40+ methods total
}
```

这是 Java 的 `EntityManager` / `SessionFactory` 的直译——一个胖 service 对象掌管一切数据访问。

### 0.2 贫血数据模型

```rust
// 当前：纯数据 struct，零行为
pub struct Entity {
    pub id: String,
    pub qualified_name: String,
    pub kind: EntityKind,
    pub origin: EntityOrigin,
    pub created_at: DateTime<Utc>,
}
// Entity 自己不知道如何验证、如何展示、如何与其他实体建立关系
// 所有行为都外化到 Store 方法和 format.rs 中
```

这是 Martin Fowler 所谓的 "Anemic Domain Model"——Rust 社区称之为 "data bags"。

### 0.3 过程式 Command = Service Layer

```rust
// 当前：每个命令是接受 &Store 的自由函数
pub fn execute(store: &Store, entity: &str, kind: AssertionKind, ..) -> Result<CommandOutput> {
    let entity_record = store.upsert_entity(..)?;
    let assertion = store.create_assertion(..)?;
    Changelog::append(store, ..)?;
    // 手动拼字符串输出
    let mut out = String::new();
    out.push_str("assertion created\n");
    // ...
    Ok(CommandOutput::success(out))
}
```

这不就是 Java 的 `@Service class AssertionService { public CommandOutput execute(..) { .. } }` 吗？

### 0.4 格式化工具类

```rust
// format.rs: 14 个公共自由函数，全部接受数据引用，返回 String
pub fn cascade_report(result: &CascadeResult) -> String { .. }
pub fn impact_report(result: &ImpactResult) -> String { .. }
pub fn query_report(entity: &Entity, assertions: &[(..)], ..) -> String { .. }
```

这就是 `StringUtils` / `ReportFormatter` ——Java 中最常见的 pattern。

### 0.5 平面模块 + 星形依赖

```
model/                    ← 所有数据 + 所有行为，扁平堆积
  store.rs  (1200行)
  types.rs  (350行)
  graph.rs  (360行)
  diff.rs   (420行)
  branch.rs (210行)
command/                  ← 每个命令依赖 Store + format
  assert_cmd.rs → store + format
  query.rs      → store + format
  verify.rs     → store + format + analysis
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

## 1. 设计原则：Rust 地道的架构哲学

在进入具体设计之前，先阐明五条从 Rust 社区共识中提炼的原则。

### 原则一：数据为王，行为跟随

> *"Structs hold data; impl blocks add behavior; traits define shared contracts."*

Rust 的 struct 首先是数据的容器。行为通过 `impl` 关联到 struct 上——不是作为"方法继承"，而是作为"该数据自然支持的操作"。如果一个操作不访问 struct 的内部字段，它应该是自由函数，不是方法。

**反例（当前）：**
```rust
// 所有操作都挂在 Store 上，即使很多操作只用到 Store 的一部分能力
store.upsert_entity(name, kind, origin)
store.create_assertion(entity_id, kind, claim, grounds, depends_on)
```

**正例（目标）：**
```rust
// 领域类型只携带纯计算行为（无持久化依赖）
let kind = EntityKind::infer(qualified_name);
let entity = repo.upsert_entity(name, kind, origin)?;

// 操作通过独立的 ops 模块完成（领域命令，非 Java Service）
let assertion = ops::assert::execute(&repo, entity, kind, claim, grounds, None)?;
```

注意：`entity.assert()` 看起来更"面向对象"，但会让 `Entity` 持有 `&dyn Repository`
引用，模糊了领域对象和持久化的边界。正确的做法是**领域类型只做纯计算，
副作用通过 Repository 在 ops 层完成**。详见 §2.3 通道 B 的论证。

### 原则二：模块是封装边界，不是文件夹

> *"A module should own a single responsibility, expose a minimal API, and hide its internals."*

Rust 的 `pub` / `pub(crate)` / `pub(super)` 提供了细粒度的可见性控制。好的模块暴露 3-5 个公共类型/函数，其余全部内部化。

**反例（当前）：**
```rust
// model/mod.rs: pub use everything
pub use branch::{BranchInfo, BranchManager};
pub use changelog::Changelog;
pub use diff::*;
pub use graph::*;
pub use store::Store;
pub use types::*;
```

### 原则三：Trait 只为真正的多态而存在

> *"Don't abstract for abstraction's sake — abstract when you have, or know you will have, more than one implementation."*

Rust 社区将这个原则称为 YAGNI（You Aren't Gonna Need It）。Trait 的代价是动态分发开销、代码可读性下降、IDE 跳转困难。除非你真的有两种实现（如真实 DB vs 内存 mock，终端输出 vs JSON 输出），否则不要引入 trait。

### 原则四：Typestate 编码进程内业务规则

> *"If a state transition is illegal, don't make it a runtime error — make it uncompilable."*

Rust 的泛型 + 零大小类型可以让状态机在编译期强制正确使用。这是 Java 做不到的，是 Rust 的独特优势。但 Typestate 有一个前提：**状态必须在同一个进程的生命周期内持续存在**。
它适用于进程内的 API 保证（如 `Experiment`），不适用于无状态的 CLI 工作流（见 §2.4.1 的论证）。

```rust
// 适用场景：进程内对象的编译期保证
let mut exp = Experiment::start(repo, "auth::login")?;  // → Open
exp.hypothesize(op)?;                                    // → Open (可继续假设)
let report = exp.evaluate()?;                            // → Evaluated
exp.commit()?;                                           // → Committed（消耗 self，不可再用）
// exp.hypothesize() after commit — won't compile
```
### 原则五：组合优于分类

> *"Compose small, focused types; resist the urge to build deep type hierarchies."*

Rust 没有继承，组合是唯一的代码复用方式。这不是限制——是优势。小的、专注的类型组合在一起比一个庞大的"基类 + 子类"树更容易理解。

---

## 2. 目标架构：六层认知模型

以下是基于你的理论框架 + Rust 地道实践重新设计的架构。六个层次从底向上，每层只依赖下层。

```
┌─────────────────────────────────────────────────────────────┐
│  第 6 层  实验层 (Experiment Layer)                         │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ Experiment, Hypothesis, ExperimentReport              │  │
│  │ "如果假设 X 成立，会发生什么？"                       │  │
│  └───────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  第 5 层  认知潜空间 (Cognitive Latent Space)               │
│  ┌──────────────────────┐ ┌──────────────────────────────┐  │
│  │ 结构子空间 (自动)    │ │ 语义子空间 (手动/Agent输入)  │  │
│  │ EntityGraph,         │ │ AssertionGraph,              │  │
│  │ Contains/Calls/Uses  │ │ TmsEngine, TraceEngine       │  │
│  │ 骨架 + 低层关系      │ │ 契约/意图/不变量/风险/修正   │  │
│  └──────────────────────┘ └──────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  第 4 层  工作流引导 (Workflow Guide)                       │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ WorkflowState: Uninit → Ready → Changing              │  │
│  │ 状态机覆盖全部 14 个命令，含建议引擎和变更循环        │  │
│  │ "现在该做什么？" — CLI 内嵌最佳实践                   │  │
│  └───────────────────────────────────────────────────────┘  │
├───────────────────────────┬─────────────────────────────────┤
│  第 3 层  自动解析        │  第 3 层  手动操作              │
│  (Analysis Pipeline)      │  (Interactive Modeling)         │
│  ┌────────────────────┐   │  ┌────────────────────────────┐ │
│  │ Scanner, Parser,   │   │  │ Assert, Retract, Depend,   │ │
│  │ Extractor,         │   │  │ Query, Index, Stats        │ │
│  │ FileWalker         │   │  │                            │ │
│  │ 确定性、零 LLM     │   │  │ Agent 驱动的认知构建       │ │
│  └────────────────────┘   │  └────────────────────────────┘ │
├───────────────────────────┴─────────────────────────────────┤
│  第 2 层  持久化抽象 (Persistence)                          │
│  ┌───────────────────────────────────────────────────────┐  │
│  │ Repository trait, SqliteRepo                          │  │
│  │ "数据如何存取"                                        │  │
│  └───────────────────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  第 1 层  代码空间 (Code Space) — 被建模的外部世界          │
│  源代码文件、目录结构、编程语言语法树                       │
│  (不属于 CLI，但 CLI 所有的操作都围绕它展开)                │
└─────────────────────────────────────────────────────────────┘
```

### 2.1 第 1 层：代码空间 (Code Space)

这不是 CLI 的一部分，但必须在架构中显式体现——因为它是所有操作的**根数据源**。

```
Code Space
├── 源文件 (src/**/*.rs, src/**/*.py, ...)
├── 目录结构 (模块层次)
├── 语言语法 (Rust, Python, JavaScript, Go, C, Java)
└── 其他结构化内容 (Cargo.toml, pyproject.toml, package.json, ...)
```

CLI 不修改代码空间——它只**读取**和**建模**它。修改由 Agent 通过编辑器完成。

### 2.2 第 2 层：持久化抽象 (Persistence)

这是唯一需要 trait 抽象的层——因为变更隔离和可测试性在此真正有价值。

```rust
/// 认知模型的持久化契约。
///
/// 只有一个实现 (SqliteRepository)，
/// 但 trait 保留了未来替换存储后端的可能性。
/// 测试使用 SqliteRepository::open_in_memory() ——真实 SQL 语义，零磁盘 I/O。
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

    // ── Transaction ──
    fn transaction<F, T>(&self, f: F) -> Result<T>
    where F: FnOnce() -> Result<T>;
}
```

关键设计决策：
- `Repository` trait 定义**数据访问**契约，不包含业务逻辑
- graph 查询作为 Repository 的方法（它们本质上是 SQL 查询）
- `transaction` 方法允许调用方控制原子性边界
- 命令只通过 `&dyn Repository` 访问数据，不知道底层是 SQLite
- **测试使用 `SqliteRepository::open_in_memory()`**，而非 InMemoryRepository

**为什么不实现 InMemoryRepository？**

SQLite 的事务隔离级别、FOREIGN KEY 约束、级联删除等语义在 HashMap 实现中难以忠实模拟。
随着业务逻辑复杂化，InMemoryRepository 和 SqliteRepository 的行为差距会越来越大，
测试会给出虚假的安全感。`rusqlite` 原生支持 `:memory:` 连接——零磁盘 I/O，速度和
HashMap 相当，但保留了 100% 的 SQL 语义保真度。

```rust
// 生产环境
let repo = SqliteRepository::open(".cog/cog.db")?;

// 测试环境——真实 SQLite，零 I/O
let repo = SqliteRepository::open_in_memory()?;
// FOREIGN KEY、事务、递归查询全部正常工作
```

### 2.3 第 3 层：双通道建模

#### 通道 A：自动解析 (Analysis Pipeline)

```rust
/// 按语言缓存 tree-sitter Parser 实例的池。
/// 避免每次解析文件时重新创建和配置 Parser（tree-sitter Parser 会复用内部分配）。
pub struct ParserPool {
    parsers: HashMap<Language, tree_sitter::Parser>,
}

impl ParserPool {
    pub fn new() -> Self { .. }
    /// 获取（或创建并缓存）指定语言的已配置 parser。
    pub fn acquire(&mut self, lang: Language) -> Result<&mut tree_sitter::Parser> { .. }
}

/// 代码空间的自动扫描器。
///
/// 职责：遍历文件系统，解析源文件，提取实体骨架 + 结构关系。
/// 确定性、零 LLM 开销、幂等。
/// 不接触 Repository——只返回纯数据，由调用方负责写入。
pub struct Scanner {
    pool: ParserPool,
    config: ScanConfig,
}

impl Scanner {
    /// 执行完整扫描，返回纯数据 ScanReport。
    pub fn scan(&mut self, root: &Path) -> Result<ScanReport> { .. }
}

/// 扫描报告：纯数据，记录发现了什么、新增了什么、过时了什么。
/// 调用方（如 init 命令）负责将其中的实体和关系写入 Repository。
pub struct ScanReport {
    pub files_scanned: usize,
    pub entities_found: usize,
    pub new_entities: Vec<Entity>,
    pub existing_entities: Vec<Entity>,
    pub new_relations: Vec<(String, String, EntityRelationKind)>,
    pub languages_detected: Vec<Language>,
}
```

关键变化：
- `ParserPool` 封装了按语言缓存的 tree-sitter Parser 实例，替代当前每次解析都 `Parser::new()` 的模式
- `scan()` **不**接受 `&dyn Repository`——返回纯数据结构，由 `init` 命令负责写入
- 保持 analysis 层对 persistence 层的零依赖
- 返回 `ScanReport` 而非裸 `ScanResult`——报告自己的行为
#### 通道 B：手动操作 (Interactive Modeling)

手动操作是 Agent 对认知模型的写入通道。每个操作是一个**领域命令**——
输入领域类型，输出领域事件，副作用通过 Repository 隔离。

**这与 Java Service Layer 的区别是什么？**

乍看之下 `ops::assert::execute(repo, ...)` 和当前的 `command::assert_cmd::execute(store, ...)`
几乎相同。但有三点本质区别：

| | 当前 (Java Service) | 目标 (领域命令) |
|---|---|---|
| **依赖方向** | 命令直接 import Store 的具体方法 | 命令只依赖 `Repository` trait |
| **输出格式** | 命令内拼 String 返回 `CommandOutput` | 命令返回领域类型（`Assertion`），格式化在外层 |
| **可测试性** | 需要 SQLite 文件 | 用 `:memory:` SQLite，无需文件系统 |
| **职责** | 验证 + 存储 + 格式化 + changelog 全做 | 只做验证 + 存储，单一职责 |

Java Service 的问题不是"函数接收参数并做事"，而是"一个方法里塞了太多不相关的职责"。
`ops/` 的函数遵循单一职责：验证输入 → 通过 Repository 执行 → 返回领域类型。
格式化和 changelog 在调用层（`cli` 命令函数）处理。

```rust
// src/ops/assert.rs — 一个模块，一个公共函数

/// 创建断言（领域命令）。
///
/// 纯粹的领域逻辑：验证 → 存储 → 返回。
/// 不负责格式化、不写 changelog、不返回 CommandOutput。
pub fn execute(repo: &dyn Repository, entity: &str,
               kind: AssertionKind, claim: &str, grounds: &str,
               depends_on: Option<&str>) -> Result<Assertion> {
    // 验证 grounds 格式
    grounds.validate_format()?;
    // 解析依赖（如果提供了短 ID）
    let resolved_dep = depends_on.map(|id| repo.resolve_short_id(id)).transpose()?;
    // 创建 entity + assertion + evidence
    let entity = repo.upsert_entity(entity, EntityKind::infer(entity), EntityOrigin::Manual)?;
    let assertion = repo.create_assertion(&entity.id, kind, claim, grounds, resolved_dep.as_deref())?;
    repo.create_evidence(&assertion.id, grounds, "manual assertion")?;
    Ok(assertion)
}
```

为什么不用 trait？因为这些操作只有一种实现。Trait 的代价是动态分发开销、代码可读性下降、
IDE 跳转困难。除非有第二种实现（如真实的 ops vs mock ops），否则 trait 是纯开销。
模块的 `pub fn` 就是最简洁的 API 边界。

### 2.4 第 4 层：工作流引导 (Workflow Guide) — 新增

这是你提到的尚未实现的功能层。核心洞察：

> **目前 CLI 接口太底层——每个命令是一个原子操作，Agent 需要自己知道调用顺序。**
> **解决：CLI 内嵌最佳实践，以状态机形式引导 Agent。**

#### 2.4.1 设计思路：命令内嵌状态管理
状态机不是一个新的命令层——每个现有命令（`init`、`query`、`assert`、`retract` 等）
在执行时自动读取 `.cog/workflow_state.json`，根据当前状态和转换规则决定是否更新状态，然后写回。
Agent 无需学习新命令，正常使用 cog 即可。
唯一新增的是 `cog next`——查看当前状态和下一步建议。
#### 2.4.2 为什么不用 Typestate？

Typestate（`Workflow<Uninit>` → `Workflow<Ready>` → `Workflow<Changed>`）在 Rust 中
是一种强大的编译期保证机制。但它有一个根本性的适用前提：**状态必须在同一个进程的
生命周期内持续存在**。

CLI 工具每次调用都是一个新进程。`cog init` 结束后，`Workflow<Ready>` 这个类型就消失了。
下一次调用 `cog assert` 时，程序从零开始，类型系统什么也不知道。
要让 Typestate 在 CLI 中工作，需要每次从 DB 重建状态——那么 Uninit/Ready/Changing
的区分就必须序列化到某个持久介质中，而 typestate 的编译期保证在这个重建过程中完全失效。

**结论：Typestate 用于库内部的 API 安全保证（如 `Experiment`），不用于 CLI 工作流。**
CLI 层的状态管理用序列化的 `WorkflowState` enum + 建议引擎。

#### 2.4.3 序列化状态机

```rust
/// 工作流状态——序列化到 .cog/workflow_state.json。
///
/// 每次命令调用时从文件加载，命令结束后写回。
/// 这不是 typestate——它是运行时状态 + 建议引擎。
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum WorkflowState {
    /// 项目未初始化（或 .cog/ 目录不存在）
    Uninit,
    /// 已初始化，模型可用
    Ready {
        phase: WorkflowPhase,
    },
    /// 正在变更中——代码已被修改，等待验证
    Changing {
        description: String,
        started_at: DateTime<Utc>,
        affected_entities: Vec<String>,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum WorkflowPhase {
    /// 刚刚 init，还没有任何 assertion
    FreshScan,
    /// 浏览、查询、记录断言——一切模型交互。无论记了多少个 assertion，都是 Exploring。
    Exploring,
    /// 正在运行 impact/trace 评估影响面
    Assessing,
    /// 代码刚刚被修改，verify 已通过，等待记录修正
    PostChange,
    /// 发现了问题——retract 触发 TMS 级联后进入，或 verify 发现不一致
    Debugging,
}
```

状态持久化：
```rust
// .cog/workflow_state.json 的内容
// { "state": "Ready", "phase": "Exploring" }
// { "state": "Changing", "description": "add rate limiting", "started_at": "...", "affected_entities": ["auth::login"] }
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

Exploring --query/assert/depend--> Exploring (不变)
Exploring --impact--> Assessing
Exploring --trace--> Assessing
Exploring --retract--> Debugging
Exploring --index/stats/export--> Exploring (不变)

Assessing --query/assert/depend--> Exploring
Assessing --index/stats/export--> Exploring
Assessing --retract--> Debugging
Assessing --impact/trace--> Assessing (不变)

PostChange --query/index/stats/export/assert/depend--> Exploring
PostChange --impact/trace--> Assessing
PostChange --retract--> Debugging

Debugging --query/assert/retract/depend--> Debugging (持续调试)
Debugging --index/stats/export--> Debugging (不变，不退出调试)
Debugging --impact/trace--> Debugging (不变)
Debugging --verify (--clean 通过)--> Exploring
Debugging --verify (未通过)--> Debugging (不变)

3. Changing 状态内的转换
Changing --verify (pass)--> Ready { PostChange }
Changing --verify (fail)--> Changing (保持，等待修复)
Changing --assert--> Changing (变更中产生的新认知，状态不变)
Changing --query/index/stats/export--> Changing (不变)
Changing --retract--> Changing (在变更中撤销相关断言)
Changing --experiment start--> Changing (不变，实验是并行的)
Changing --finish-change--> Ready { Exploring }
Changing --abort-change--> Ready { Exploring }

4. 无状态影响的命令 (任何 Ready/Changing 均可)
depend, delete-entity, index, stats, export --> 不改变状态
experiment start/discard/commit --> 不改变顶级状态（实验是并行子会话）
```

**关键设计决策：**

1. **Exploring 涵盖一切模型交互**：查询、断言记录、依赖管理都在 Exploring 中进行。不再用计数器区分"探索"和"建模"——Agent 记第一个 assertion 和记第 50 个都是"在构建模型"。

2. **`retract` 总是进入 Debugging**：retract 触发 TMS 级联，可能让下游断言变成 `Uncertain`。无论当前在哪个 Ready phase，retract 后都应该进入 Debugging 模式。

3. **Debugging 的退出条件严格**：只有 `verify --clean` 通过才能退出到 Exploring。`export` / `index` / `stats` 等浏览操作**不退出** Debugging——浏览不等于修好了。

4. **`impact` / `trace` 进入 Assessing**：这两个命令是评估工具，使用它们表示 Agent 正在评估一个潜在变更的影响面。

5. **`init` 仅在 Uninit 下可用**：已初始化的项目再次 init 会报错，防止误操作重置模型。

6. **`assert` 在 Changing 下保持 Changing**：变更过程中 Agent 可能产生新认知（如发现新的 fragility），这是合理的，不改变变更状态。

7. **`experiment` 与 workflow 并行**：Experiment 是独立的推理通道，不影响 workflow 的顶级状态。

8. **`delete-entity` 不改变状态但触发建议**：删除实体是破坏性操作，建议引擎会在下一轮 `cog next` 中建议 `verify`。

#### 2.4.4 建议引擎
```rust
/// 根据当前工作流状态 + 模型数据，返回 Agent 可以执行的操作列表。
/// Agent 只需调用 `cog next` 就能知道"下一步该做什么"。
pub fn suggest_actions(state: &WorkflowState, repo: &dyn Repository) -> Vec<SuggestedAction> {
    match state {
        WorkflowState::Uninit => vec![SuggestedAction {
            action: ActionKind::InitProject,
            description: "No cognitive model found. Run init to scan the codebase.".into(),
            why: "Without a structural model, cog cannot provide guidance.".into(),
            example_command: "cog init .".into(),
        }],
        WorkflowState::Ready { phase } => suggest_for_ready(phase, repo),
        WorkflowState::Changing { description, affected_entities, .. } => {
            suggest_for_changing(description, affected_entities, repo)
        }
    }
}
fn suggest_for_ready(phase: &WorkflowPhase, repo: &dyn Repository) -> Vec<SuggestedAction> {
    let mut actions = Vec::new();
    let stats = repo.stats().unwrap_or_default();
    match phase {
        WorkflowPhase::FreshScan => {
            if stats.assertion_count == 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::StartRecording,
                    description: format!("{} entities found but 0 assertions.", stats.entity_count),
                    why: "Entities without assertions are 'unknown unknowns' during changes.".into(),
                    example_command: "cog query <core_entity>".into(),
                });
            }
            let orphans = repo.count_unasserted_entities().unwrap_or(0);
            if orphans > 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::RecordMissingContracts { entity_count: orphans },
                    description: format!("{} entities have no assertions yet.", orphans),
                    why: "Core modules need contracts before any change.".into(),
                    example_command: "cog assert <entity> --kind contract --claim \"...\"".into(),
                });
            }
        }
        WorkflowPhase::Exploring => {
            let orphans = repo.count_unasserted_entities().unwrap_or(0);
            if orphans > 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::RecordMissingContracts { entity_count: orphans },
                    description: format!("{} entities have no assertions yet.", orphans),
                    why: "Core modules need contracts before any change.".into(),
                    example_command: "cog assert <entity> --kind contract --claim \"...\"".into(),
                });
            }
            actions.push(SuggestedAction {
                action: ActionKind::AssessImpact { entity: "try a core entity".into() },
                description: "Run impact analysis to understand downstream dependencies.".into(),
                why: "Knowing blast radius before changes reduces surprise.".into(),
                example_command: "cog impact <core_entity>".into(),
            });
        }
        WorkflowPhase::Assessing => {
            actions.push(SuggestedAction {
                action: ActionKind::StartChange,
                description: "Begin a code change now that you've assessed impact.".into(),
                why: "Impact assessment is most useful just before making a change.".into(),
                example_command: "cog start-change \"<description>\"".into(),
            });
            actions.push(SuggestedAction {
                action: ActionKind::StartExperiment,
                description: "Or, run a what-if experiment before committing to a change.".into(),
                why: "Experiments let you test hypotheses safely without modifying the codebase.".into(),
                example_command: "cog experiment start <entity>".into(),
            });
        }
        WorkflowPhase::PostChange => {
            actions.push(SuggestedAction {
                action: ActionKind::RecordFix { entity: "changed entity".into() },
                description: "Record corrections for changed entities.".into(),
                why: "Keep the model in sync with the code after changes.".into(),
                example_command: "cog assert <entity> --kind correction --claim \"...\"".into(),
            });
            actions.push(SuggestedAction {
                action: ActionKind::StartChange,
                description: "Begin another change cycle if more work remains.".into(),
                why: "Ready for the next iteration.".into(),
                example_command: "cog start-change \"<description>\"".into(),
            });
        }
        WorkflowPhase::Debugging => {
            let uncertain = repo.count_uncertain_assertions().unwrap_or(0);
            if uncertain > 0 {
                actions.push(SuggestedAction {
                    action: ActionKind::ReviewUncertainAssertions { count: uncertain },
                    description: format!("{} assertions are uncertain since last retraction.", uncertain),
                    why: "Uncertain assertions have weakened ground — they need re-verification.".into(),
                    example_command: "cog query <affected_entity>".into(),
                });
            }
            actions.push(SuggestedAction {
                action: ActionKind::TraceRootCause,
                description: "Trace dependency chains to find root causes.".into(),
                why: "TMS cascade may have weakened downstream assertions.".into(),
                example_command: "cog trace <entity>".into(),
            });
            actions.push(SuggestedAction {
                action: ActionKind::VerifyConsistency,
                description: "Run verify to check structural consistency.".into(),
                why: "Retraction may have left orphaned dependencies.".into(),
                example_command: "cog verify --scan".into(),
            });
        }
    }
    // Ready 状态总是可以进入变更模式
    actions.push(SuggestedAction {
        action: ActionKind::StartChange,
        description: "Begin a code change with impact assessment.".into(),
        why: "Always assess impact before modifying code.".into(),
        example_command: "cog start-change \"<description>\"".into(),
    });
    actions
}
fn suggest_for_changing(
    description: &str, affected: &[String], repo: &dyn Repository
) -> Vec<SuggestedAction> {
    let mut actions = vec![SuggestedAction {
        action: ActionKind::VerifyChanges,
        description: format!("Verify model consistency after: {}", description),
        why: "Changes may violate existing contracts or invariants.".into(),
        example_command: "cog verify".into(),
    }];
    if !affected.is_empty() {
        actions.push(SuggestedAction {
            action: ActionKind::RecordFix { entity: affected[0].clone() },
            description: format!("Record fix for {}", affected[0]),
            why: "Document corrections to keep the model accurate.".into(),
            example_command: format!("cog assert {} --kind correction --claim \"...\"", affected[0]),
        });
    }
    actions.push(SuggestedAction {
        action: ActionKind::StartExperimentDuringChange,
        description: "Run a what-if experiment to test a fix before committing.".into(),
        why: "Experiments let you verify hypotheses without modifying the real model.".into(),
        example_command: "cog experiment start <affected_entity>".into(),
    });
    actions.push(SuggestedAction {
        action: ActionKind::FinishChange,
        description: "Finish this change cycle.".into(),
        why: "All fixes recorded. Return to normal operation.".into(),
        example_command: "cog finish-change".into(),
    });
    actions.push(SuggestedAction {
        action: ActionKind::AbortChange,
        description: "Abort this change and discard tracking.".into(),
        why: "You can always abandon a change cycle and return to Ready state.".into(),
        example_command: "cog abort-change".into(),
    });
    actions
}
```

```rust
/// CLI 给 Agent 的建议——"下一步你可以做什么"。
pub struct SuggestedAction {
    pub action: ActionKind,
    pub description: String,
    pub why: String,
    pub example_command: String,
}

pub enum ActionKind {
    /// 项目未初始化，需要先 init
    InitProject,
    /// 有一些孤立实体，应该记录它们的契约
    RecordMissingContracts { entity_count: usize },
    /// 有一些 uncertain 的断言等待复核
    ReviewUncertainAssertions { count: usize },
    /// 检测到代码变更，断言可能过时
    ReverifyAfterChange { changed_files: Vec<String> },
    /// 刚刚完成 init，建议开始记录核心模块的契约
    StartRecording,
    /// 运行 impact 评估
    AssessImpact { entity: String },
    /// 即将修改一个有很多下游的核心实体
    WarnHighImpact { entity: String, downstream_count: usize },
    /// 变更中，需要验证
    VerifyChanges,
    /// 变更验证通过，可以记录修正
    RecordFix { entity: String },
    /// 变更完成，可以结束
    FinishChange,
    /// 放弃当前变更跟踪
    AbortChange,
    /// 追溯根因
    TraceRootCause,
    /// 运行一致性验证
    VerifyConsistency,
    /// 开始一个实验（假设推理）
    StartExperiment,
    /// 在变更过程中开始实验
    StartExperimentDuringChange,
    /// 开始一个新的变更
    StartChange,
}
```

#### 2.4.5 CLI 暴露
状态管理内嵌在现有命令中——Agent 不需要 `workflow` 前缀。正常使用 cog 即可，`cog next` 查看建议：
```bash
# Agent 问：现在该做什么？
$ cog next
Suggested actions:
  1. [record_contracts] 32 entities have no assertions yet. Start with core modules.
     Why: Entities without assertions are "unknown unknowns" during changes.
     Example: cog assert <entity> --kind contract --claim "..."

  2. [assess_impact] Run impact analysis to understand downstream dependencies.
     Why: Knowing blast radius before changes reduces surprise.
     Example: cog impact src::model::store::Store

  3. [start_change] Begin a code change with impact assessment.
     Why: Always assess impact before modifying code.
     Example: cog start-change "add rate limiting to login"

# 正常使用流程——命令自带状态管理
$ cog init .
$ cog query auth::login
$ cog assert auth::login --kind contract --claim "..." --grounds "..."
$ cog impact auth::login              # → 状态进入 Assessing
$ cog start-change "add rate limiting to login"
$ cog verify
$ cog assert auth::login --kind correction --claim "now rate-limited at 5 req/sec"
$ cog finish-change

# 调试流程：retract 触发 Debugging
$ cog retract <assertion-id> --reason "signature changed"
→ 3 downstream assertions marked Uncertain.
→ State: Ready { Debugging }
$ cog next
→ Suggested: review 3 uncertain assertions, trace root cause, verify consistency

# 错误恢复：放弃当前变更跟踪
$ cog abort-change
→ Change "add rate limiting" aborted. Model state preserved.
→ State changed: Changing → Ready { Exploring }
```

### 2.5 第 5 层：认知潜空间 (Cognitive Latent Space)

潜空间有两个子空间——它们共享 entity 作为锚点，但数据类型和操作截然不同。

#### 2.5.1 结构子空间 (自动)

```
EntityGraph:
  - Entity 节点: { id, qualified_name, kind, origin, metrics }
  - 关系边:     contains, calls, uses
  - 来源:       tree-sitter 扫描 → 自动构建
  - 更新:       cog init → 增量同步
```

关键类型：

```rust
/// 结构子空间的只读视图。
///
/// 从 Repository 加载所需子图，纯内存操作，不持有数据库连接。
pub struct StructureSpace {
    entities: HashMap<String, EntityNode>,
    edges: Vec<StructureEdge>,
}

impl StructureSpace {
    /// 从 Repository 加载以 root_entity 为中心的子图（BFS，深度可配置）
    pub fn load(repo: &dyn Repository, root: &str, depth: usize) -> Result<Self> { .. }

    /// 查找所有直接依赖 entity 的节点（入边）
    pub fn dependents_of(&self, entity: &str) -> Vec<&EntityNode> { .. }

    /// 查找 entity 直接依赖的所有节点（出边）
    pub fn dependencies_of(&self, entity: &str) -> Vec<&EntityNode> { .. }

    /// 两个 entity 之间的最短路径
    pub fn shortest_path(&self, from: &str, to: &str) -> Option<Vec<&EntityNode>> { .. }
}
```

#### 2.5.2 语义子空间 (手动/Agent 输入)

```
AssertionGraph:
  - Assertion 节点: { id, kind, status, claim, entity_id }
  - Evidence 节点:  { id, source, detail, assertion_id }
  - 关系边:         depends_on (Assertion → Assertion)
                    has_evidence (Assertion → Evidence)
  - 来源:           cog assert → 手动构建
  - 更新:           cog retract → TMS 级联
```

关键类型：

```rust
/// 语义子空间——TMS 信念维护系统。
/// 从 Repository 加载断言子图到内存，提供纯数据结构的分析与模拟能力。
/// 不执行真实的持久化操作——真实撤销在 `ops::retract::execute` 中完成。
pub struct SemanticSpace {
    assertions: HashMap<String, AssertionNode>,
    evidence: HashMap<String, EvidenceNode>,
    depends_on: Vec<(String, String)>,      // (dependent, dependency)
}
impl SemanticSpace {
    pub fn load(repo: &dyn Repository, entity_id: &str) -> Result<Self> { .. }
    /// 模拟撤销：计算级联影响，返回受影响断言清单。
    /// 纯函数——不修改 Repository，仅操作内存数据。
    /// 用于 Experiment 评估和 impact 分析。
    pub fn simulate_retract(&self, assertion_id: &str) -> CascadeReport {
        // BFS 沿 depends_on 反向边级联
        // 算法从 Store 中提取出来，变成纯数据结构操作
    }
    /// 沿 depends_on 链追溯到根因
    pub fn trace(&self, assertion_id: &str) -> TraceTree { .. }
    /// 综合评估修改 entity 的风险
    pub fn assess_risk(&self, entity: &str, structure: &StructureSpace) -> RiskAssessment { .. }
}
```
**关键设计变化：**
- 真实的断言撤销由 `ops::retract::execute(repo, id, reason)` 执行（写 SQLite + 触发 changelog）
- `SemanticSpace::simulate_retract` 是纯内存模拟——加载子图 → 计算级联 → 返回报告，不碰数据库
- 两者分离后：Experiment 用模拟评估影响，CLI `retract` 命令执行真实撤销
- 图算法（BFS 级联、DFS 追溯）成为纯数据结构操作，可单元测试而不依赖 SQLite

### 2.6 第 6 层：实验层 (Experiment Layer)

> *"在潜空间中推演，而不是在代码空间中试错"*

实验层是一个统一的推理环境，融合了原设计中的 Sandbox（轻量快速评估）和 Branch（跨 session 持久化推理）两种需求。核心洞察：

> **认知模型不需要 git 风格的分支——需要的是科学实验。**
> 你不会 "fork" 一个理论然后合并它——你提出假设，检验，然后接受或拒绝。

#### 2.6.1 为什么淘汰 Branch？

原 Branch 设计（`VACUUM INTO` 复制整个 SQLite 文件 → 独立操作 → diff/merge）存在四个结构性问题：

1. **Merge 语义薄弱**：实体删除在 merge 时被跳过（"would break cross-references"），merge 是有损近似合并，不是可靠的合并。
2. **UUID 共享导致合并歧义**：两个分支同时修改同一断言的 claim 时，只能手动 `--apply` / `--reject`，比 git 的三路合并更弱。
3. **开销不对称**：创建分支是完整 DB 复制，而绝大多数"如果"推理只需要局部子图快照。
4. **和 Sandbox 功能重叠**：两者的隔离实验语义高度重叠，区分仅在于"生命周期长度"和"容量大小"——这不是本质区别。

Branch 的正确定位：**降级为全量模型备份工具**（`cog backup`），用于大规模架构变更前的安全网。不再作为面向用户的实验接口。

#### 2.6.2 统一的 Experiment 设计
Experiment 是**单根假设推理工具**——围绕一个不确定的变更点，模拟它的传播后果。
多实体大规模重构不属于 Experiment 的职责，应使用 `cog backup` 做全量备份安全网。
```rust
/// 推理实验——围绕单个 entity 的假设性推演。
///
/// 一个实验 = 基态内存快照 + 假设性操作列表 + 评估报告。
/// 实验可以跨 session 序列化恢复。
/// 这是 typestate 真正适用的场景：
/// Experiment 在同一个进程内创建、操作、提交或丢弃，
/// 编译期保证"未提交就丢弃"或"提交后不再使用"。
pub struct Experiment {
    id: ExperimentId,
    description: String,
    created_at: DateTime<Utc>,
    /// 基态：从 Repository 加载的结构和语义子图（内存快照，非完整 DB 复制）
    structure: StructureSpace,
    semantic: SemanticSpace,
    /// 假设性操作列表（操作日志，非最终状态）
    staged: Vec<ExperimentOp>,
    status: ExperimentStatus,
}

pub enum ExperimentStatus {
    Open,
    Evaluated,
    Committed,
    Discarded,
}
/// 实验中的假设性操作
pub enum ExperimentOp { .. }  // 见下文
```
关键设计：
**1. 轻量级快照——自适应深度 + 边界检测**
```rust
impl Experiment {
    /// 从某个实体开始一个实验——加载其依赖子图到内存。
    /// 深度自适应：沿依赖边 BFS 直到自然边界，或子图节点数超过阈值（默认 500）。
    /// 记录被截断的边界实体，评估报告中标注"可能不完整"。
    pub fn start(repo: &dyn Repository, entity: &str, description: &str) -> Result<Self> {
        // BFS 收集子图，深度由实际依赖拓扑决定
        let (structure, boundary) = StructureSpace::load_adaptive(repo, entity, max_nodes=500)?;
        let semantic = SemanticSpace::load(repo, entity)?;
        Ok(Experiment {
            id: ExperimentId::new(),
            description: description.into(),
            created_at: Utc::now(),
            structure,
            semantic,
            staged: Vec::new(),
            status: ExperimentStatus::Open,
            boundary_entities: boundary,  // 被截断的实体——评估报告需标注
        })
    }
    /// 注入假设性操作
    pub fn hypothesize(&mut self, op: ExperimentOp) -> Result<()> { .. }
    /// 评估假设性变更的影响——使用 SemanticSpace::simulate_retract 等模拟方法
    pub fn evaluate(&self) -> Result<ExperimentReport> { .. }
    /// 提交——replay staged 操作到主 DB（确定性回放）
    pub fn commit(self) -> Result<CommitReport> { .. }
    /// 丢弃所有假设
    pub fn discard(self) { /* drop */ }
}
```
**2. Snapshot 是什么？** Space 类型本身就是 snapshot——从 Repository 加载到内存的数据副本。
不再需要 `SubgraphSnapshot` 中间包装类型。Experiment 直接持有 `StructureSpace` 和 `SemanticSpace`。

**3. 跨 session 持久化——不需要 VACUUM INTO**

```rust
// 实验序列化到 .cog/experiments/<id>.json
// 不复制整个 DB，只序列化操作日志和快照摘要
impl Experiment {
    pub fn save(&self, path: &Path) -> Result<()> { .. }
    pub fn load(repo: &dyn Repository, path: &Path) -> Result<Self> { .. }
    pub fn list_saved(experiments_dir: &Path) -> Result<Vec<ExperimentSummary>> { .. }
}
```

```bash
# 启动实验
$ cog experiment start auth::login --desc "what if login takes 3 params?"

# 注入假设
$ cog experiment hypothesize --assert auth::login \
    --kind contract --claim "now accepts (username, password, rate_limit)" \
    --grounds "hypothesis:rate-limit-feature"

# 评估
$ cog experiment evaluate
Risk: High (0.82)
Affected: 7 assertions
Contradictions: api::login_handler expects 2 params

# 保存到磁盘（跨 session 恢复）
$ cog experiment save

# ... 下次 session ...
$ cog experiment load <id>
$ cog experiment evaluate     # 重新评估（从保存的状态恢复）
$ cog experiment commit       # 或 discard

# 列出所有保存的实验
$ cog experiment list
  exp_a1b2  Open       "what if login takes 3 params?"  2026-06-05
  exp_c3d4  Committed  "hypothesize async Store"         2026-06-04
  exp_e5f6  Discarded  "test removing session module"    2026-06-03
```

**4. commit 语义：replay 操作日志**

与 Branch merge 不同，Experiment commit 是确定性的 replay：
- `HypotheticalAssertion` → 在真实 DB 上执行 `create_assertion`
- `HypotheticalRetraction` → 在真实 DB 上执行 `retract` + TMS 级联
- `HypotheticalRelation` → 在真实 DB 上执行 `add_entity_relation`
- `HypotheticalDelete` → 在真实 DB 上执行 `delete_entity`

replay 是确定性的——因为记录的是操作（intent），不是修改后的状态（state discrepancy）。这比 diff-then-merge 更可靠，且不需要 UUID 冲突解决。

```rust
/// 实验结果报告
pub struct ExperimentReport {
    /// 受影响的断言（级联传播结果）
    pub affected_assertions: Vec<AffectedAssertion>,
    /// 新引入的矛盾
    pub contradictions: Vec<Contradiction>,
    /// 风险评分：0.0 (安全) → 1.0 (高风险)
    pub risk_score: f64,
    /// 人类可读的摘要
    pub summary: String,
}
```

#### 2.6.3 Experiment 与状态机的交互
Experiment 是状态机的**并行子会话**——它不影响顶级 `WorkflowState`，但在建议引擎中会被推荐：
- `Ready { Assessing }` → 建议 `StartExperiment`：评估完影响面后，可以在实验里验证假设
- `Changing` → 建议 `StartExperimentDuringChange`：在变更中做快速假设验证
- `Ready { Debugging }` → 建议 `StartExperiment`：用实验测试不同的修复方案
实验提交后，如果涉及断言的增删改，建议引擎会在下一轮 `cog next` 中检测到变化并建议相应的操作（如 `verify` 或重新 assert）。

#### 2.6.4 Branch 的降级定位

Branch 保留为底层备份能力，重命名为 `backup`：

```bash
# 全量模型备份（替代原 branch create）
$ cog backup create --name "before-major-refactor"

# 列出备份
$ cog backup list

# 恢复备份（替代原 branch switch）
$ cog backup restore "before-major-refactor"

# 删除备份
$ cog backup drop "before-major-refactor"
```

备份是完整 DB 副本（`VACUUM INTO`），适合大范围架构变更前的安全网。日常的假设推理走 Experiment。

---

## 3. 新的模块结构

> 以下为设计目标结构。实际实施见上方实施状态表及偏差说明。

```
src/
├── main.rs                    # 入口点：解析 CLI，装配依赖，分发
├── cli.rs                     # Clap 定义（保持简洁，不包含业务逻辑）
├── domain/                    # ── 领域核心（不依赖外部库除了 std）──
│   ├── mod.rs
│   ├── entity.rs              # Entity, EntityKind, EntityOrigin
│   ├── assertion.rs           # Assertion, AssertionKind, AssertionStatus
│   ├── evidence.rs            # Evidence
│   ├── relations.rs           # EntityRelation, AssertionRelation, relation kinds
│   ├── changelog.rs           # ChangelogEntry, ChangelogAction
│   ├── metrics.rs             # EntityMetrics (line_count, fan_in, fan_out, visibility) [TODO]
│   └── grounds.rs             # Grounds 格式验证 newtype
├── repo/                      # ── 持久化层 ──
│   ├── mod.rs
│   ├── trait.rs               # Repository trait
│   └── sqlite.rs              # SqliteRepository (impl Repository, 含 open_in_memory)
├── analysis/                  # ── 自动解析管道 ──
│   ├── mod.rs
│   ├── scanner.rs             # Scanner: 遍历 + 调度
│   ├── walker.rs              # FileWalker: 文件系统遍历
│   ├── pool.rs                # ParserPool: 按语言缓存 tree-sitter Parser
│   ├── extractors/            # 语言特定提取器
│   │   ├── mod.rs
│   │   ├── rust.rs
│   │   ├── python.rs
│   │   ├── javascript.rs
│   │   ├── go.rs
│   │   ├── c.rs
│   │   └── java.rs
│   └── report.rs              # ScanReport
├── ops/                       # ── 手动操作（每个操作一个模块）──
│   ├── mod.rs
│   ├── assert.rs              # 创建断言
│   ├── retract.rs             # 撤销断言 + TMS 级联（真实持久化操作）
│   ├── depend.rs              # 记录实体关系
│   ├── query.rs               # 查询实体认知卡片
│   ├── index.rs               # 实体索引
│   ├── stats.rs               # 模型统计
│   ├── verify.rs              # 一致性验证
│   ├── export.rs              # 模型导出
│   └── entity.rs              # 实体删除
├── space/                     # ── 潜空间操作（内存分析层）──
│   ├── mod.rs
│   ├── structure.rs           # StructureSpace (结构子空间)
│   ├── semantic.rs            # SemanticSpace (语义子空间，含 simulate_retract)
│   ├── cascade.rs             # CascadeReport (纯图算法)
│   ├── impact.rs              # ImpactCard (BFS 影响面)
│   ├── trace.rs               # TraceTree (DFS 依赖链)
│   └── risk.rs                # RiskAssessment (综合评估)
├── experiment/                # ── 实验层 ──
│   ├── mod.rs
│   ├── session.rs             # Experiment 主类型 (typestate)
│   ├── ops.rs                 # ExperimentOp
│   ├── report.rs              # ExperimentReport, Contradiction, CommitReport
│   └── persistence.rs         # 序列化/恢复 (跨 session)
├── backup/                    # ── 全量模型备份（原 Branch 降级）──
│   ├── mod.rs
│   └── manager.rs             # BackupManager: create/list/restore/drop
├── workflow/                  # ── 工作流状态机（新增）──
│   ├── mod.rs
│   ├── state.rs               # WorkflowState enum (序列化到 .cog/workflow_state.json)
│   ├── suggestions.rs         # SuggestedAction, ActionKind, 建议引擎
│   └── best_practice.rs       # 内置最佳实践规则
├── format/                    # ── 输出格式化 ──
│   ├── mod.rs
│   └── text.rs                # TextRenderer (人类可读文本输出)
```
### 3.1 依赖方向 (编译期强制)
```
main ──→ cli ──→ ops ──→ space ──→ repo ──→ domain
        │        │
        │        ├──→ workflow (状态文件读写)
        │        ├──→ analysis ──→ repo ──→ domain
        │        ├──→ experiment ──→ space ──→ repo ──→ domain
        │        └──→ backup ──→ repo ──→ domain
        │
        └──→ format
```
每层只依赖直接在它下面的层。`cli` 编排 `ops`、`workflow`、`analysis`、`experiment`、`backup`，但它们之间不互相依赖。

---

## 4. 关键类型的重新设计

### 4.1 `Entity`：从 Data Bag 到 Domain Type

```rust
/// 认知模型中被建模的最小单元。
///
/// 不变量：
/// - qualified_name 非空
/// - id 是有效 UUID v4
/// - kind 与 name 的命名约定一致
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    id: EntityId,             // newtype: 保证 UUID v4 格式
    qualified_name: QualifiedName, // newtype: 保证 "::" 分隔的非空段
    kind: EntityKind,
    origin: EntityOrigin,
    metrics: EntityMetrics,
    created_at: DateTime<Utc>,
}

impl Entity {
    /// 从扫描创建实体。origin 自动设为 Scan。
    pub fn from_scan(name: QualifiedName, kind: EntityKind, metrics: EntityMetrics) -> Self { .. }

    /// 从手动断言创建实体。origin 自动设为 Manual。
    pub fn from_manual(name: QualifiedName, kind: EntityKind) -> Self { .. }

    /// 实体的短名称（最后一段）。
    pub fn short_name(&self) -> &str { self.qualified_name.last_segment() }

    /// 实体所属的模块名称。
    pub fn module(&self) -> Option<&str> { self.qualified_name.parent() }

    /// 此实体是否为公开 API（从 metrics.visibility 推断）。
    pub fn is_public(&self) -> bool { self.metrics.visibility.is_public() }
}
```

```rust
/// 限定名：`::` 分隔的路径段，如 `cog::model::store::Store`。
/// 非空，每段至少一个字符。解析由构造函数保证。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QualifiedName(String);
impl QualifiedName {
    pub fn parse(raw: &str) -> Result<Self> { .. }
    /// 最后一段，如 `Store`
    pub fn last_segment(&self) -> &str { .. }
    /// 父路径，如 `cog::model::store`
    pub fn parent(&self) -> Option<&str> { .. }
    pub fn segments(&self) -> impl Iterator<Item = &str> { .. }
}
```
**关键改变：**
- `id` 不是裸 `String`，是 `EntityId` newtype——防止与 `AssertionId` 混淆
- `qualified_name` 不是裸 `String`，是 `QualifiedName`——自带解析、比较、格式化
- `Entity::from_scan` / `from_manual` 内部调用 `Uuid::new_v4()` 生成 ID——调用方无需关心 UUID
- `short_name()`, `module()` 是固有方法，不是外部的工具函数
- `EntityMetrics`（`line_count`, `fan_in`, `fan_out`, `visibility`）是后续迭代的优化项 [TODO]，当前设计预留字段但不阻塞实现
### 4.2 `Store` → `Repository` trait + `SqliteRepository`

当前的 `Store` 被拆为：

```rust
// 1. Repository trait — 数据访问契约
pub trait Repository: Send + Sync {
    // ... 见 2.2 节
}

// 2. SqliteRepository — 唯一实现（生产 + 测试共用）
pub struct SqliteRepository {
    conn: rusqlite::Connection,
}

impl SqliteRepository {
    /// 生产环境：打开磁盘上的数据库
    pub fn open(path: &Path) -> Result<Self> { .. }

    /// 测试环境：打开内存数据库（零磁盘 I/O，完整 SQL 语义）
    pub fn open_in_memory() -> Result<Self> { .. }
}

impl Repository for SqliteRepository {
    // 所有 SQL 细节封装在此
}
```

**为什么不实现 InMemoryRepository？** 见 §2.2 的详细论证。
核心原因：`HashMap` 无法忠实模拟 FK 约束、事务隔离、级联删除。
SQLite `:memory:` 保留 100% SQL 语义保真度，且速度与内存 HashMap 相当。

**注意：不引入 ORM，不引入连接池，不引入复杂的事务抽象。** Cog 是单用户 CLI 工具，不是 web 服务。`rusqlite::Connection` 直接使用。简洁优先。
### 4.3 格式化：`Serialize` + `Renderable` + `TextRenderer`
**方案：报告类型 derive `Serialize` + 实现 `Renderable` trait，文本格式由 `TextRenderer` 处理。**
```rust
/// 输出格式
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Text,
    Json,
    // 未来可扩展: Dot, Markdown
}
/// 报告类型可实现此 trait 以支持文本渲染
pub trait Renderable {
    fn render_text(&self) -> String;
}
// 报告类型同时是数据载体和序列化单元
#[derive(Debug, Clone, Serialize)]
pub struct QueryCard {
    pub entity: Entity,
    pub assertions: Vec<Assertion>,
    pub related: Vec<RelatedEntity>,
}
impl Renderable for QueryCard {
    fn render_text(&self) -> String {
        TextRenderer::query_card(self)
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct CascadeReport {
    pub retracted: Assertion,
    pub affected: Vec<AffectedAssertion>,
    pub cascade_depth: usize,
}
impl Renderable for CascadeReport {
    fn render_text(&self) -> String {
        TextRenderer::cascade_report(self)
    }
}
// TextRenderer 是唯一知道"如何给人类看"的组件
pub struct TextRenderer;
impl TextRenderer {
    pub fn query_card(card: &QueryCard) -> String { .. }
    pub fn cascade_report(report: &CascadeReport) -> String { .. }
    pub fn impact_card(card: &ImpactCard) -> String { .. }
}
// CLI 层的输出路由
fn emit_report<T: Serialize + Renderable>(report: &T, output: OutputFormat) {
    match output {
        OutputFormat::Text => println!("{}", report.render_text()),
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(report).unwrap()),
    }
}
```
### 4.4 聚合查询返回类型
```rust
/// Repository::stats() 的返回值
#[derive(Debug, Clone, Serialize)]
pub struct ModelStats {
    pub entity_count: usize,
    pub assertion_count: usize,
    pub evidence_count: usize,
    pub relation_count: usize,
}
```

---

## 5. 工作流引导：状态机详细设计

状态转换规则和建议引擎的完整设计见 §2.4。本节提供命令→状态映射表和 CLI 示例。

### 5.1 完整命令→状态映射表

| 命令 | 适用状态 | 目标状态 | 副作用 |
|------|---------|---------|--------|
| `init .` | Uninit | Ready { FreshScan } | 扫描代码库，创建模型 |
| `init .` | Ready, Changing | 错误: "already initialized" | — |
| `query <e>` | Ready { FreshScan } | Ready { Exploring } | 首次查询进入探索 |
| `query <e>` | Ready { any other } | 不变 | — |
| `assert <e> ...` | Ready { FreshScan } | Ready { Exploring } | — |
| `assert <e> ...` | Ready { Exploring/Assessing/PostChange } | Ready { Exploring } | — |
| `assert <e> ...` | Changing | Changing (不变) | 变更中产生新认知，合理 |
| `retract <id>` | Ready { any } | Ready { Debugging } | TMS 级联触发 |
| `retract <id>` | Changing | Changing (不变) | 变更中撤销相关断言 |
| `impact <e>` | Ready { any except Debugging } | Ready { Assessing } | — |
| `impact <e>` | Ready { Debugging } | Ready { Debugging } (不变) | — |
| `trace <e>` | Ready { Exploring/PostChange } | Ready { Assessing } | — |
| `trace <e>` | Ready { Debugging } | Ready { Debugging } (不变) | 调试中持续追溯 |
| `depend <a> --on <b>` | Ready { any } | 不变 | — |
| `index` | Ready { any } | 不变 | — |
| `stats` | Ready { any } | 不变 | — |
| `export` | Ready { any } | 不变 | — |
| `delete-entity <e>` | Ready { any } | 不变 | 下次 `cog next` 建议 verify |
| `verify` | Changing (pass) | Ready { PostChange } | 通过则退出变更 |
| `verify` | Changing (fail) | Changing (不变) | 保持变更，等待修复 |
| `verify` | Ready { Debugging } (--clean 通过) | Ready { Exploring } | 唯一退出 Debugging 的路径 |
| `verify` | Ready { Debugging } (未通过) | Ready { Debugging } (不变) | — |
| `verify` | Ready { any other } | 不变 | 发现 inconsistency 时 `cog next` 建议 retract |
| `start-change "<desc>"` | Ready { any } | Changing | — |
| `finish-change` | Changing | Ready { Exploring } | — |
| `abort-change` | Changing | Ready { Exploring } | 放弃跟踪 |
| `next` | 任何状态 | 不变 | 只读，显示建议 |
| `experiment start <e>` | Ready { any }, Changing | 不变 | 并行子会话 |
| `experiment commit` | Ready { any }, Changing | 不变 | commit 后触发下一轮建议更新 |
| `experiment discard` | Ready { any }, Changing | 不变 | — |

### 5.2 典型使用流程

```bash
# 初始化
$ cog init .

# 查看建议
$ cog next
→ Suggested: record contracts for 128 entities, assess impact, start change

# 正常使用——命令自带状态管理
$ cog query src::model::store::Store
$ cog assert src::model::store::Store --kind contract --claim "..." --grounds "..."
$ cog impact auth::login              # → 状态进入 Assessing
$ cog start-change "add rate limiting"
$ cog verify
$ cog assert auth::login --kind correction --claim "now rate-limited"
$ cog finish-change

# retract 触发调试
$ cog retract <id> --reason "signature changed"
→ 3 downstream assertions marked Uncertain. State: Debugging.
$ cog next
→ Suggested: review uncertain assertions, trace root cause, verify

# 错误恢复
$ cog abort-change
→ Change aborted. State: Ready { Exploring }
```
---

## 6. 迁移路线图

架构重设计是一个大工程。以下是一个增量迁移路径，每个阶段都可以独立交付。

> **实施进度 (2026-06-05):** Phase 1-4 的核心架构重构（module 拆分、Repository trait、WorkflowState、TextRenderer）已完成。剩余的是类型精细化（Phase 1 的 newtype）和新增功能层（Phase 5-6 的 experiment/backup）。


### Phase 1：核心类型重构（不改外部行为）

- [ ] `Entity.id` → `EntityId` newtype — 放弃，String 足够
- [ ] `Entity.qualified_name` → `QualifiedName` newtype — 放弃，改用 last_segment()/parent_qname() 自由函数
- [ ] `Assertion.id` → `AssertionId` newtype — 放弃，String 足够
- [ ] `Grounds` newtype 带格式验证
- [ ] 为 `Entity`, `Assertion` 添加固有方法（`short_name()`, `module()` 等）
- [ ] 删除对应的自由函数

**影响面：** 所有使用 `entity.id` 和 `entity.qualified_name` 的地方。但编译器会精确指出每一处。

### Phase 2：Repository trait 提取

- [ ] 从 `Store` 中提取 `Repository` trait
- [ ] 将 `Store` 重命名为 `SqliteRepository` 并实现 `Repository`
- [ ] 添加 `SqliteRepository::open_in_memory()` 用于测试
- [ ] 所有命令改为接受 `&dyn Repository`
- [ ] 更新所有测试，使用 `:memory:` 连接替代临时文件

**影响面：** 所有命令的签名。`Store` → `dyn Repository`。

### Phase 3：图算法与存储解耦

- [ ] 创建 `space/` 模块
- [ ] `CascadeResult` → `CascadeReport`，操作在 `StructureSpace` + `SemanticSpace`
- [ ] `ImpactResult` → `ImpactCard`，同上
- [ ] `TraceResult` → `TraceTree`，同上
- [ ] 算法从 `Store` 依赖改为纯数据结构操作

**影响面：** `graph.rs` 完全重写。所有调用方更新。

### Phase 4：状态机引导
- [ ] 创建 `workflow/` 模块（状态机逻辑）
- [ ] 实现 `WorkflowState` enum（序列化到 `.cog/workflow_state.json`）
- [ ] 每个现有命令内部集成状态读取/更新逻辑
- [ ] 实现建议引擎（纯函数，覆盖全部 5 个 WorkflowPhase）
- [ ] 新增命令：`cog next`, `cog start-change`, `cog finish-change`, `cog abort-change`
- [ ] 现有命令保持不变，但自动获得状态感知能力
**影响面：** 新增功能。现有命令添加状态文件读写逻辑，接口不变。

### Phase 5：实验层（统一替换 Sandbox + Branch）

- [x] 创建 `experiment/` 模块
- [x] 实现 `Experiment`（typestate：Open → Evaluated → Committed/Discarded）
- [x] 实现 `ExperimentOp`：HypotheticalAssertion, HypotheticalRetraction, HypotheticalRelation, HypotheticalDelete
- [x] 实现 `ExperimentReport` 和 `CommitReport`
- [x] 实现序列化/恢复（`.cog/experiments/<id>.json`）
- [x] 添加 `cog experiment` 子命令（start, hypothesize, evaluate, commit, discard, save, load, list）
- [x] 实现 commit 的 replay 语义（操作回放，非 diff-merge）
- [x] 将 `branch/` 降级为 `backup/`（cog backup create/list/restore/drop）
- [x] 保留旧分支文件格式的可读性（向后兼容）

**影响面：** `branch/` 模块降级为 `backup/`。新增 `experiment/` 模块。旧分支文件仍然可读，`branch` 命令可选保留或标记 deprecated。

### Phase 6：格式化重构

- [ ] 报告类型 derive `Serialize`
- [ ] 实现 `TextRenderer`
- [ ] 添加 `--output` 参数（text / json）
- [ ] `format.rs` 逐渐退役

**影响面：** 所有命令的输出构造方式。

---

## 7. 设计决策记录

| 决策 | 选择 | 理由 |
|------|------|------|
| Repository 用 trait 还是具体类型？ | trait | 这是唯一一个真正需要多态的地方（为将来存储后端留接口）。其他层不需要 trait。 |
| 测试用 InMemoryRepository 还是 SQLite `:memory:`？ | SQLite `:memory:` | InMemoryRepository 无法忠实模拟 FK 约束、事务隔离、级联删除等 SQL 语义，长期维护成本高且给出虚假安全感。`:memory:` 保留 100% SQL 保真度。 |
| 图算法用纯函数还是方法？ | 方法（在 Space 类型上） | `space.simulate_retract(id)` 比自由函数更清晰地表达操作语义。 |
| 撤销的真实执行 vs 模拟评估？ | `ops::retract::execute`（真实）+ `SemanticSpace::simulate_retract`（模拟） | 真实撤销写 SQLite + 触发 changelog；模拟是纯内存计算，用于 Experiment 和 impact 分析。两者职责不同，不能合并在一个方法里。 |
| 状态管理如何暴露？ | 现有命令内嵌状态管理 | 不引入 `workflow` 子命令前缀。每个命令自动读取/更新状态文件，Agent 正常使用 cog 即可感知状态引导。新增仅 `cog next`, `cog start-change`, `cog finish-change`, `cog abort-change`。 |
| 用不用 typestate？ | 序列化 enum（CLI），typestate（Experiment） | CLI 每次调用是新进程，typestate 的编译期保证无法跨进程传递。Experiment 在同一进程内创建/提交/丢弃，typestate 适用。 |
| Branch 与 Experiment 的关系？ | Experiment 单根假设推理 + Backup 全量备份 | Experiment 围绕一个不确定点做推理；多实体大规模重构用 Backup 做安全网。Branch 降级为 backup。 |
| Experiment commit 用 diff-merge 还是 replay？ | replay 操作日志 | 操作日志是确定性回放，不需要 UUID 冲突解决。比 diff-then-merge 更简单可靠。 |
| Experiment snapshot 深度？ | 自适应 BFS + 边界检测 | 不再硬编码 depth=3。沿依赖边 BFS 直到自然边界或节点数超阈值（默认 500），记录被截断的边界实体。 |
| 格式化用 Display 还是 Serialize + Renderer？ | `Serialize` + `Renderable` trait + `TextRenderer` | `Display` 锁死单一输出格式。`Renderable` trait + 独立 `TextRenderer` 使加 `--output json/dot/markdown` 是加法。 |
| `ops/` 函数与 Java Service 的区别？ | 领域命令 | 单一职责（验证 + 存储）、无格式化、只依赖 Repository trait。Java Service 的问题是职责过多。 |
| `anyhow` 还是自定义错误？ | `anyhow` 保留 | CLI 工具不需要细粒度错误匹配。domain 层的类型验证用 `thiserror` 提供清晰消息。 |
| 工作流状态覆盖多少个 Phase？ | 5 个：FreshScan, Exploring, Assessing, PostChange, Debugging | 去掉了 Modeling（与 Exploring 语义重叠）。Exploring 涵盖一切模型交互。 |
| `init` 可否重复调用？ | 仅在 Uninit 下可用 | Ready/Changing 状态下 `init` 报错 "already initialized"，防止误操作重置模型。 |

---

## 8. 与现有设计文档的关系

本文档是 `docs/COGNITIVE_MODEL_DESIGN.md` 的**架构实现层对应**。两者关系：

| COGNITIVE_MODEL_DESIGN.md | 本文档 |
|---------------------------|--------|
| 理论与数学基础（TMS, 潜空间, 级联算法） | Rust 地道的工程实现 |
| "是什么" 和 "为什么" | "怎么做" 和 "怎么做对" |
| Entity/Assertion/Evidence 的语义定义 | Entity/Assertion/Evidence 的 Rust 类型设计 |
| 潜空间的四个层次 | StructureSpace + SemanticSpace 的具体结构 |
| 分支作为推理工具 | Experiment 单根推理 + Backup 全量备份 |
| 未涉及 | 状态机引导（5 Phase 覆盖全部命令，建议引擎内嵌到现有命令） |
两份文档共同构成 Cog 的完整设计：一份定义**理论**，一份定义**实现**。

## 附录 A：参考文献与进一步阅读

- [Rust Design Patterns](https://rust-unofficial.github.io/patterns/) — 官方社区编目的 Rust 模式、惯用法和反模式
- [Rust Is Beyond Object-Oriented](https://www.thecodedmessage.com/posts/oop-1-encapsulation/) — The Coded Message 的系列文章，解释 Rust 如何超越 OOP
- [Trait-Driven Rust Architecture](https://github.com/raminfp/trait-driven-rust-architecture) — 演示了 trait 驱动架构的示例项目
- [Rust DDD Kickstart](https://github.com/Serrucia/Rust-DDD-kickstart) — 领域驱动设计在 Rust 中的实践模板
- [Zero To Production In Rust](https://www.zero2prod.com/) — Luca Palmieri 的 Rust 后端开发书籍
- [CLI/Web Service Architecture Discussion](https://users.rust-lang.org/t/cli-web-service-architecture-am-i-doing-it-wrong/91246) — Rust 论坛关于架构的讨论
- [entrait](https://docs.rs/entrait/latest/entrait/) — 一个帮助在 Rust 中实现依赖注入的 crate
