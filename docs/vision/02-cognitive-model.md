# 认知模型：理论框架

> 本文定义 cog 的核心概念。这些是跨实现稳定的理论原语；具体 Rust 类型见 [architecture/02-domain-layer.md](../architecture/02-domain-layer.md)，存储 schema 见 [reference/02-data-model.md](../reference/02-data-model.md)。

## 1 三个基元

认知模型由三种基本元素构成：

| 基元 | 含义 | 代码域对应 | 例子 |
|------|------|-----------|------|
| **Entity** | 被认知的对象 | 代码符号（模块、函数、类型、字段、方法） | `auth::login` |
| **Assertion** | 关于 Entity 的一个信念 | 契约、意图、不变量、脆弱点、修正 | "login 在密码错误时返回 None" |
| **Evidence** | 支撑 Assertion 的观察 | 源码引用、测试、运行时观测 | `code:auth::login` |

## 2 两类关系

基元通过两类有向关系连接。

**实体关系**（自动提取或手动声明，`EntityRelationKind`）：

```
Entity ──contains──▶ Entity   组合：模块包含函数
Entity ──calls─────▶ Entity   运行时调用：函数 A 调用函数 B
Entity ──uses──────▶ Entity   结构依赖：函数 A 使用类型 B
```

**认知关系**（agent 推理构建，`AssertionRelationKind`，目前仅一种）：

```
Assertion ──depends_on──▶ Assertion   逻辑依赖：A 成立则 B 才成立
```

`assertion → evidence` 的支撑关系在概念上存在，但在 schema 中 evidence 通过 `assertion_id` 外键直接归属 assertion，不作为独立的关系边建模。

## 3 五种断言分类

`AssertionKind`：

| 分类 | 含义 | 生命周期 | 例子 |
|------|------|---------|------|
| **Contract** | 行为契约：接受什么、保证什么 | 接口变更时更新 | "接受非空切片，返回排序结果" |
| **Intent** | 设计意图：为什么这样做 | 重大重构时可能更新 | "用归并排序是因为需要稳定排序" |
| **Invariant** | 不变量：什么必须永远成立 | 一旦确立很少变 | "id 字段构造后永远为正数" |
| **Fragility** | 已知风险：什么地方脆弱 | 修复后可由 correction 取代 | "浮点比较没有 eps，边界可能出错" |
| **Correction** | 修正记录：上次哪里错了 | append-only，永远保留 | "上次改成 try-catch 是错的，根因是上游没校验" |

## 4 四个核心性质

### 可构建（正向推理）

从观察出发，沿 `depends_on` 边逐步推导新 Assertion：

```
读到 fn login(pwd: &str) -> Option<Token>
  → Entity("auth::login")
  → Assertion(contract): "接受密码字符串，返回 Option<Token>"
      grounds: code:auth::login
  → Assertion(contract): "返回 None 意味着认证失败"
      depends_on: ↑, grounds: test:tests/auth.rs::test_login_fail
```

### 可追溯（反向推理）

从被违反的 Assertion 出发，沿 `depends_on` 回溯，检查每条 Evidence 直到定位不成立的那个。`cog trace` 实现此路径的可视化（DFS，见 [architecture/05-space-layer.md](../architecture/05-space-layer.md)）。

### 可更新（TMS 级联）—— 核心机制

当 Evidence 与 Assertion 矛盾时，`retract` 沿 `depends_on` 边 BFS 传播影响。**算法基于 Doyle (1979) 的 Truth Maintenance System**。

精确语义（重要——见 [reference/03-tms-cascade.md](../reference/03-tms-cascade.md)）：

1. 标记被证伪的 Assertion 为 `Retracted`。
2. BFS 遍历所有下游 dependent。
3. 对每个 dependent，检查它是否还有**其它独立的 Active 依赖**：
   - **有** → 报告为 `GroundWeakened`（报告原因），**断言仍保持 `Active`**——它还有别的支撑。
   - **没有** → 标记为 `Uncertain`（存储状态变更），继续 BFS 传播。

> **关键澄清**：`GroundWeakened` **不是**存储状态，只是级联报告里的 `CascadeReason`。存储的 `AssertionStatus` 只有三种：`Active`、`Retracted`、`Uncertain`。归档文档曾错误地将 `GroundWeakened` 描述为第四种状态。

`Uncertain` 的断言可通过 `cog recover` 在其全部依赖重新变为 `Active` 后恢复——见 [reference/01-cli-reference.md](../reference/01-cli-reference.md)。

### 唯一（单一活跃状态）

模型在任意时刻只有一个活跃状态。实验（experiment）机制允许在内存快照上推演假设，但主模型始终唯一。所有写操作追加到 append-only changelog。

## 5 双层结构

Entity 网络在概念上分两层（实现上由 `EntityOrigin` 区分，见 [reference/02-data-model.md](../reference/02-data-model.md)）：

```
┌─────────────────────────────────────────────┐
│  自动层（tree-sitter 扫描，origin = Scan）  │
│  文件/目录包含、函数/类型定义、import、调用 │
│  特点：确定性、零 LLM 开销、结构化          │
└────────────────────┬────────────────────────┘
                     │ 骨架
┌────────────────────┴────────────────────────┐
│  理解层（agent 推理构建，origin = Manual）  │
│  契约、意图、不变量、脆弱点、修正           │
│  特点：语义性、需要 agent、渐进积累         │
└─────────────────────────────────────────────┘
```

自动层由 `cog sync` 构建（幂等、可重复），理解层由 `cog assert` 渐进积累。

## 6 粒度控制

并非每个代码符号都需要 Assertion。判定标准："如果这个假设被违反，会不会导致难以定位的 bug？"

| 层级 | 是否记录 | 判定标准 |
|------|---------|---------|
| 模块职责 | 必定记录 | 模块的存在目的 |
| 公开接口契约 | 必定记录 | 调用方依赖的行为保证 |
| 关键不变量 | 必定记录 | 违反会导致难以定位的 bug |
| 设计意图 | 选择性记录 | 非显而易见的决策 |
| 已知风险 | 选择性记录 | 已识别但暂未修复的边界 |
| 内部辅助函数 | 通常不记录 | 标准模式、样板代码 |
