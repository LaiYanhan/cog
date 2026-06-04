# Coding Agent 认知模型设计文档

> 日期：2026-06-03
> 状态：已实现（cog v0.1.0）
> 范围：为 LLM Coding Agent 构建外部化认知模型的理论框架与工程实现

---

## 1 问题定义

### 1.1 LLM 代码的腐败问题

人类开发者编写的代码具有一种抽象的**稳定性**：开发者对代码存在唯一且稳定的认知——至少能够回忆起"当时为什么在这里用这种方式写了代码"。这种认知模型潜藏在人类大脑中，支撑着代码的长期维护。

LLM 生成的代码缺乏这种稳定性。同一个 LLM 在上一个 session 中写下的代码，在下一个 session 中可能产生完全不同的解释。这种认知不连续导致：

- **短视修补**：遇到报错时不去溯源根因，而是在表象层面打补丁
- **认知漂移**：对同一代码块的"理解"随 session 变化，缺乏一致性
- **腐败加速**：每次修改都可能引入与先前决策意图矛盾的改变
- **归因缺失**：无法回答"为什么这里是这样而不是那样"

### 1.2 问题本质

LLM 的根本缺陷不是代码质量，而是**没有持久的外部认知模型**。人类维护代码时依赖的是大脑中隐含的认知结构（因果链、不变量、设计意图、修正历史），而 LLM 在每次交互时都从零开始构建理解。

> **核心论点**：为 LLM 构建一个外部化的认知模型——可构建、可追溯、可更新、且唯一——可以从根本上解决 LLM 代码的腐败问题。

---

## 2 理论框架

### 2.1 三个基元

认知模型由三种基本元素构成：

| 基元 | 含义 | 代码域对应 | 例子 |
|------|------|-----------|------|
| **Entity** | 被认知的对象 | 代码符号（模块、函数、类型、字段） | `UserService::login()` |
| **Assertion** | 关于 Entity 的一个信念 | 代码契约、设计意图、不变量 | "login 在密码错误时返回 None" |
| **Evidence** | 支撑或反驳 Assertion 的观察 | 源码文本、测试结果、编译输出 | "测试 test_login_fail 通过" |

### 2.2 两类关系

基元之间通过两类有向关系连接：

**实体关系（自动提取或手动声明）：**

```
Entity ──contains──▶ Entity      组合：模块包含函数
Entity ──calls────▶ Entity      调用：函数 A 调用函数 B
Entity ──uses─────▶ Entity      依赖：函数 A 使用类型 B
```

**认知关系（LLM 推理构建）：**

```
Entity ──has──────▶ Assertion    归属：关于这个实体的信念
Assertion ──grounds──▶ Evidence  证据链：为什么相信这个断言
Assertion ──depends──▶ Assertion 逻辑依赖：A 成立则 B 才成立
```

### 2.3 Assertion 的五种分类

| 分类 | 含义 | 生命周期 | 例子 |
|------|------|---------|------|
| **Contract** | 行为契约：接受什么、保证什么 | 最稳定，接口变更时更新 | "接受非空切片，返回排序结果" |
| **Intent** | 设计意图：为什么这样做 | 重大重构时可能更新 | "用归并排序是因为需要稳定排序" |
| **Invariant** | 不变量：什么必须永远成立 | 一旦确立很少变 | "id 字段构造后永远为正数" |
| **Fragility** | 已知风险：什么地方脆弱 | 修复后可能降级或消除 | "浮点比较没有 eps，边界可能出错" |
| **Correction** | 修正记录：上次哪里错了 | append-only，永远保留 | "上次改成 try-catch 是错的，根因是上游没校验" |

### 2.4 四个核心性质

#### 可构建（正向推理）

从观察出发，沿 `depends` 边逐步推导出新的 Assertion：

```
Observation: 读到 fn login(pwd: &str) -> Option<Token>
  → Entity("login 函数")
  → Assertion("login 接受密码字符串，返回 Option<Token>")
    grounds: Evidence("源码第 42 行的函数签名")
  → Assertion("返回 None 意味着认证失败")
    depends_on: ↑
    grounds: Evidence("Option 类型的语义约定")
```

#### 可追溯（反向推理）

从违反的 Assertion 出发，沿 `depends` 边回溯，检查每条 Evidence 直到定位不成立的那个：

```
Observation: "login 返回 None，但密码是正确的"
  → 匹配违反的 Assertion: "返回 None 意味着认证失败"
  → follows depends 回溯:
    Assertion("login 接受密码字符串") → grounds 检查 → 成立 ✓
    Assertion("密码比对使用 constant_time_eq") → grounds 检查
      → Evidence("源码第 47 行") → 实际是 == 而非 constant_time_eq
  → 定位: Assertion "使用安全比对" 的 Evidence 是错误的
```

#### 可更新（修正）

当 Evidence 与 Assertion 矛盾时，沿 `depends_on` 边 BFS 传播影响：

1. 标记被证伪的 Assertion 为 Retracted
2. BFS 找到所有下游 Assertion
3. 有其他独立支撑的下游 → 保留，标记 GroundWeakened
4. 唯一支撑是被证伪者的下游 → 标记 Uncertain
5. 基于新 Observation 创建修正后的 Assertion

此算法基于 Doyle (1979) 的 **Truth Maintenance System (TMS)**。

#### 唯一（单一活跃状态）

模型在任意时刻只有一个活跃状态。分支机制允许创建快照用于推理推演，但主模型始终唯一。所有历史保留在 append-only changelog 中。

---

## 3 Agent 适配

### 3.1 双层架构

Entity 网络分为两层：

```
┌─────────────────────────────────────────────┐
│              自动层（tree-sitter 扫描）     │
│                                             │
│  文件包含关系    函数定义    类型定义       │
│  import/use     模块结构    公开接口        │
│                                             │
│  特点：确定性、零 LLM 开销、结构化          │
└────────────────────┬────────────────────────┘
                     │ 骨架
┌────────────────────┴────────────────────────┐
│              理解层（LLM 推理构建）         │
│                                             │
│  函数契约      设计意图      已知不变量     │
│  边界风险      修正历史      隐式假设       │
│                                             │
│  特点：语义性、需要 LLM、渐进积累           │
└─────────────────────────────────────────────┘
```

自动层通过 `cog init` 构建，理解层通过 `cog assert` 渐进积累。

### 3.2 粒度控制

不是每个代码符号都需要 Assertion。核心判定标准："如果这个假设被违反，会不会导致难以定位的 bug？"

| 层级 | 是否记录 | 判定标准 |
|------|---------|---------|
| 模块职责 | **必定记录** | 模块的存在目的 / 核心使命 |
| 公开接口契约 | **必定记录** | 调用方依赖的行为保证 |
| 关键不变量 | **必定记录** | 违反会导致难以定位的 bug |
| 设计意图 | 选择性记录 | 非显而易见的决策 |
| 已知风险 | 选择性记录 | 已识别但暂未修复的边界 |
| 内部辅助函数 | 通常不记录 | 标准模式、样板代码 |

### 3.3 Agent 工作流

```
1. 收到任务
   └→ cog query: 获取涉及 Entity 的认知上下文

2. 规划变更
   └→ cog impact: 改动可能影响哪些 Entity 和 Assertion

3. 实现变更
   └→ 遵守已知 Invariant，不违反 Contract

4. 验证变更
   └→ 对受影响 Assertion 重新收集 Evidence
   └→ 如有失败 → 进入 5

5. 追溯根因
   └→ cog trace: 沿 depends 链回溯，定位被违反的 Assertion
   └→ 检查每条 Evidence，找到第一个不成立的

6. 修正认知
   └→ cog retract: 撤回错误 Assertion + 级联更新
   └→ cog assert: 修正后的新 Assertion + Correction 记录

7. 持久化
   └→ 模型写入 SQLite，跨 session 保持
```

### 3.4 Bootstrap 策略

模型随实践渐进积累，与人类加入新团队的过程一致：

```
Session 1: 模型为空
  → Agent 处理第一个 task
  → 构建涉及的 Entity 和 Assertion

Session 2: 模型有少量内容
  → 部分可复用已有模型
  → 新增涉及的 Entity，可能修正已有 Assertion

Week 2: 模型逐渐丰满
  → Agent 对核心模块已有完整认知
  → 新 task 大量复用已有理解
  → 模型开始发挥"认知稳定性"作用
```

---

## 4 工程实现

### 4.1 架构

```
┌───────────────────────────────────────────────────┐
│            Coding Agent (LLM)                     │
│                                                   │
│  职责：语义判断                                   │
│  - 读代码，理解在做什么                           │
│  - 做出决策，写出代码                             │
│  - 遇到 bug 时判断哪个假设错了                    │
│                                                   │
│  通过 CLI 工具配合 SKILL 指南与认知模型交互       │
└──────────────┬────────────────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────────┐
│          Cognitive Model (cog)              │
│                                             │
│  职责：结构维护                             │
│  - 图存储与查询 (SQLite)                    │
│  - 依赖追踪 (BFS/DFS)                       │
│  - 级联更新 (retract → cascade)             │
│  - 一致性验证 (verify)                      │
│  - Changelog (append-only audit trail)      │
│  - 分支 (推理推演快照)                      │
└─────────────────────────────────────────────┘
```

**分工原则**：LLM 负责"理解"和"判断"，程序负责"记忆"和"一致性"。

### 4.2 存储

SQLite 单文件数据库。选择理由：

| 选择理由 | 说明 |
|---------|------|
| 图遍历 | SQL 递归查询支持 BFS/DFS |
| 事务保证 | 级联更新不能改一半失败 |
| 零依赖 | 嵌入式，无需独立服务进程 |
| Git 友好 | 单文件，可版本化 |

### 4.3 CLI 接口

**查询类：**

```bash
cog query <entity>              # 实体的断言 + 1-hop 关系
cog impact <entity>             # 变更影响面（BFS 下游）
cog trace <entity>              # 完整依赖链 + evidence 树
cog index [--kind] [--origin] [--prefix]  # 实体索引
cog stats                       # 模型统计
```

**写入类：**

```bash
cog assert <entity> --kind <kind> --claim "<claim>" --grounds "<source:detail>" [--depends-on <id>]
cog retract <id> --reason "<why>"          # 自动级联更新下游
cog depend <entity-a> --on <entity-b> --kind <calls|uses|contains>
cog delete-entity <entity>                 # 级联删除所有关联数据
```

**验证类：**

```bash
cog verify [--scope <prefix>] [--clean] [--scan] [--scan-path <path>]
cog export [--format json|toml|dot]
```

**分支：**

```bash
cog branch create --name <name>   # 创建模型快照
cog branch switch <name>          # 切换到分支
cog branch diff <name>            # 对比主模型与分支
cog branch merge <name> [--apply-all]  # 合并分支到主模型
cog branch drop <name>            # 丢弃分支
```

**初始化：**

```bash
cog init [path] [--dry-run] [--depth <n>] [--lang <langs>]
```

### 4.4 Token 效率

| 操作 | 场景 | 估算 Token 消耗 |
|------|------|----------------|
| `cog query` | 任务开始，获取上下文 | 200-500 |
| `cog impact` | 变更前，了解影响面 | 100-300 |
| `cog trace` | 遇到 bug，追溯根因 | 200-500 |
| `cog assert` | 记录新认知 | 50 (确认) |
| `cog retract` | 修正认知 | 100-200 (含受影响清单) |
| `cog verify` | 验证一致性 | 100-300 |
| **每任务总计** | | **≈ 1000-2000** |

对比全量 dump 方案（15000-30000 tokens/任务），查询接口将模型交互的 token 开销降低了一个数量级。

### 4.5 级联更新

`retract` 触发的级联是确定性图算法（不依赖 LLM）：

```
retract(assertion_id, reason):
    标记 assertion 为 Retracted(reason)
    BFS 沿 depends_on 反向边遍历所有下游：
      - 有其他独立 Active 依赖 → GroundWeakened（保留 Active）
      - 唯一依赖是被 retract 者 → Uncertain（继续 BFS）
    changelog 追加操作记录
    返回受影响列表供 LLM 复核
```

级联策略只有两种结果：**GroundWeakened**（有其他支撑，保留但标记弱化）和 **Uncertain**（唯一支撑断裂）。区分依据是图的结构（是否存在其他独立 Active 依赖边），而非边的语义标签。这个二态模型在实践中已足够——无论哪种状态，agent 的下一步都是"检查受影响断言，决定修正或确认"。

降低依赖遗漏风险的机制：

| 机制 | 原理 |
|------|------|
| 自动结构依赖 | Entity 间的调用/包含关系由 tree-sitter 提取，不经 LLM |
| Conservative default | 不确定时宁可标 depend，过度连接比遗漏安全 |
| 影响面预检 | retract 返回下游清单，LLM 复核是否有遗漏 |
| 周期性验证 | `cog verify` 检查所有 Active Assertion 的结构一致性 |

---

## 5 方案对比

### 5.1 与树搜索方案的对比

| 维度 | 树搜索（原方案） | 认知模型（本方案） |
|------|----------------|------------------|
| 数据结构 | 多分支树（N 个候选） | 单一图（1 个活跃状态） |
| 评价方式 | 连续评分 → 需要可靠信号 | 证伪驱动 → 只需二元信号 + 解释 |
| 知识表示 | 每个 node 独立的元数据 | 全局共享、相互关联的信念网络 |
| 跨 session | 无持久化（每次重新搜索） | 持久化（模型随实践演化） |
| 根因分析 | 无（只知"分数低"） | 有（沿依赖链回溯到错误假设） |
| 学习能力 | 无（不积累经验） | 有（Correction 历史避免重复犯错） |
| LLM 开销 | 高（每 node 需完整推理） | 低（复用已有认知） |

### 5.2 为什么评分方案不可行

原方案依赖连续评分信号来引导搜索，但在 coding agent 场景中：

- Agent 自己写的测试 → 几乎必然满分（天花板效应，信号方差 ≈ 0）
- LSP 诊断 → 跨轮次的中间状态无意义，最终状态几乎必然无错
- LLM 自评 → 不独立，自我肯定偏差
- 项目已有测试 → 唯一可靠的外部信号，但多数项目不充分

结论：在 agent 自主写实现 + 自主写测试的范式下，不存在可靠的自动化连续评分信号。这不是调参能解决的问题，是信息论约束。

认知模型方案绕过了这个约束：用证伪（二元信号）替代评分（连续信号），用解释（哪条假设错了）替代分数（多高多低）。

---

## 6 端到端示例

### 6.1 场景

一个 Rust Web 项目，Agent 首次接触，接到任务："给 login 函数添加登录频率限制"。

### 6.2 Session 1：构建初始认知

```
Agent 收到任务
  → cog query auth::login
  → 返回: (空，模型尚未构建)
  → Agent 读代码 src/auth.rs

Agent 理解代码后:
  → cog assert auth::login --kind Contract \
      --claim "接受 (&str, &str) 用户名密码，返回 Option<Token>"
      --grounds "code:auth::login"

  → cog assert auth::login --kind Intent \
      --claim "密码比对使用 constant_time_eq 防止 timing 攻击"
      --grounds "code:auth::login"

  → cog assert auth::login --kind Invariant \
      --claim "同一用户连续失败 5 次后锁定账户 15 分钟"
      --grounds "code:auth::login"

  → cog assert AuthToken --kind Invariant \
      --claim "token 字段构造后为非空字符串"
      --grounds "code:AuthToken"

  → cog assert auth::login --kind Contract \
      --claim "返回 None 意味着认证失败"
      --grounds "test:tests/auth.rs:test_login_fail"
      --depends-on <login_contract_id>
```

### 6.3 Session 2：利用认知 + 修正

```
Agent 收到新任务: "用户反馈有时登录成功但拿不到 Token"

  → cog query auth::login
  → 返回: 5 条 Assertion + 依赖关系

Agent 分析: Token 返回 None 但密码正确 → 可能是 Token 构造问题
  → cog impact auth::login
  → 返回: 下游依赖 [AuthToken, session_handler]

Agent 检查代码，发现问题: 并发场景下 Token 生成有竞争条件

  → cog trace auth::login
  → 返回依赖链:
    Assertion("token 字段构造后为非空字符串")
    → grounds: code:AuthToken
    → 但实际: 并发下 Token::new 可能返回空 token

  → cog retract <token_nonempty_id> --reason "并发竞争导致 Token 可能为空"

  → 自动级联: 标记依赖 "token 字段构造后为非空字符串" 的下游为 Uncertain

  → cog assert AuthToken --kind Correction \
      --claim "Token::new 需要加锁或使用原子操作，并发下可能生成空 token"
      --grounds "runtime_error:并发测试发现空 token"

  → 修正代码，添加同步机制

  → cog verify --scope auth
  → 所有 Assertion 验证通过
```

### 6.4 Session 3：复用认知

```
Agent 收到任务: "给 register 函数添加密码强度校验"

  → cog query auth::register
  → 返回: 注册函数的已有 Assertion

  → cog impact auth::register
  → 返回: 影响面

Agent 发现模型中有:
  - Assertion("密码使用 bcrypt 哈希，cost factor 12")
  - Assertion("密码强度校验在哈希之前执行")
  → 直接基于这些认知规划变更，无需重新阅读所有代码

实现后:
  → cog assert auth::register --kind Contract \
      --claim "密码必须包含大小写字母和数字，最少 8 位"
      --grounds "task_requirement:密码强度规范"
```

---

## 7 价值分析与局限性

> 本节基于 cog v0.1.0 的实际使用经验，特别是对 cog 自身进行自我建模（self-bootstrapping）的实践反思。

### 7.1 潜空间

认知模型本质上是将原始代码空间压缩为一个**可推理的潜空间**。这个潜空间由四个层次构成：

| 层次 | 来源 | 信息类型 | 类比 |
|------|------|----------|------|
| 结构层 | tree-sitter 扫描 | entity 名称、kind、containment 层级 | 骨架 |
| 关系层 | import 解析 + manual depend | contains/calls/uses 有向边 | 连接 |
| 语义层 | LLM assert | contract/intent/invariant/fragility/correction | 理解 |
| 时序层 | changelog | 操作审计轨迹 | 历史 |

### 7.2 不可替代价值

一个自然的问题是：随着基础模型能力增强，直接读代码就能完成所有分析，cog 的价值何在？答案在于三个基础模型做不到的特性：

**推理持久化。** cog 存的不是事实（"merge_branch 存在于 branch_cmd.rs，54 行，调用 apply_item"），而是推理链——intent、fragility、correction、设计决策。推理链携带因果结构，agent 读到断言时不需要重新理解"为什么这个改过"。

**TMS 级联。** 当知识被推翻时（retract），系统沿 `depends_on` 边 BFS，自动识别所有受影响的推理。没有任何静态分析工具或基础模型能在 session 之间做到这件事。TMS 级联将"知识变动的影响评估"从非确定性的推理问题变成了确定性的图算法。

**潜空间将规划变成图搜索。** 没有 cog 时，"如果我要改 Store，会影响什么？"是一个需要逐文件阅读的推理问题。有了 cog 后，这个问题变成 `cog impact model::store::Store`——一个 O(V+E) 的 BFS 遍历，与代码库大小线性相关，与 agent 的注意力窗口无关。

### 7.3 什么不是 cog 的价值

- **不是静态分析器。** 行数统计、圈复杂度、代码相似度是 SonarQube 的领地。
- **不是代码索引。** ctags、LSP 已经做得足够好。cog 的 entity 列表是推理的锚点，不是代码浏览的替代。
- **不是文档生成器。** cog 的断言是给 agent 的推理输入，不是给人读的 API 文档。

### 7.4 当前局限性

**实体元数据稀疏。** tree-sitter 扫描只提取名称和 kind。`Store::stats`（55 行的重构方法）和 `Store::get_entity`（15 行的简单查询）在潜空间中信息密度相同。缺少实现复杂度的信号。

**查询返回数据而非推理。** `cog query X` 返回 X 的断言和关系列表。agent 真正想问的是"改 X 安全吗？"或"X 有什么历史教训？"。这些综合查询需要 agent 手动组合多个命令。

**depends_on 使用率低。** 即使有完整的 `--depends-on` 支持，实践中 agent 极少建立断言间的依赖链。断言间关系是更高阶的建模行为，大多数场景下 ROI 不够。这意味着 TMS 级联的威力在实践中尚未充分发挥。

### 7.5 待验证的假设

| 假设 | 风险 | 验证方式 |
|------|------|---------|
| LLM 能可靠地提取 Assertion | LLM 可能生成模糊或错误的断言 | 实际 task 实验，衡量 assertion 准确率 |
| 依赖图足以捕获隐式耦合 | 某些依赖可能是隐式的（如性能特征） | 对比 LLM 标注的 depends_on 与实际耦合 |
| 查询子图足够指导决策 | 可能需要多跳才能覆盖所有相关信息 | 衡量 task 完成质量与查询深度的关系 |
| 渐进 bootstrap 足够有效 | 早期 task 可能因模型不完整而质量下降 | 对比有/无模型的 task 完成质量 |

---

## 附录 A：术语表

| 术语 | 定义 |
|------|------|
| Entity | 认知模型中代表代码构造体的节点（函数、类型、模块等） |
| Assertion | 关于 Entity 的一条认知断言（契约、意图、不变量等） |
| Evidence | 支撑或反驳 Assertion 的观察事实 |
| depends_on | Assertion 间的逻辑依赖关系 |
| grounds | Assertion 与其支撑 Evidence 间的关系 |
| Retract | 标记 Assertion 为已撤回，并级联更新下游 |
| Correction | 记录一次认知修正的 append-only 历史条目 |
| TMS | Truth Maintenance System，信念维护系统 |
| Cascade | 沿依赖链传播的级联更新操作 |

## 附录 B：关键参考

- Doyle, J. (1979). "A Truth Maintenance System". *Artificial Intelligence*, 12(3), 231-272.
- de Kleer, J. (1986). "An Assumption-based TMS". *Artificial Intelligence*, 28(2), 127-162.
- Friston, K. (2010). "The Free-Energy Principle: A Unified Brain Theory?" *Nature Reviews Neuroscience*, 11(2), 127-138.
- Popper, K. (1959). *The Logic of Scientific Discovery*. Hutchinson.
- Yao, S. et al. (2023). "Tree of Thoughts: Deliberate Problem Solving with Large Language Models". NeurIPS 2023.
- Hamm, T. & Ajanovic, E. (2026). "Tree of Thoughts as a Classical Heuristic Search Problem". arXiv:2605.28566.
