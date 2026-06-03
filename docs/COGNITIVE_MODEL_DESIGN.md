# Coding Agent 认知模型设计报告

> 日期：2026-06-01
> 状态：设计阶段
> 范围：为 LLM Coding Agent 构建外部化认知模型的理论框架与工程方案

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

## 2 认知模型的理论框架

### 2.1 三个基元

认知模型由三种基本元素构成：

| 基元 | 含义 | 代码域对应 | 例子 |
|------|------|-----------|------|
| **Entity** | 被认知的对象 | 代码符号（模块、函数、类型、字段） | `UserService::login()` |
| **Assertion** | 关于 Entity 的一个信念 | 代码契约、设计意图、不变量 | "login 在密码错误时返回 None" |
| **Evidence** | 支撑或反驳 Assertion 的观察 | 源码文本、测试结果、编译输出 | "测试 test_login_fail 通过" |

### 2.2 两类关系

基元之间通过两类有向关系连接：

**实体关系（自动提取，无需 LLM）：**

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

人类对代码的认知是多维度的。一个函数，人类同时持有几种不同性质的信念：

| 分类 | 含义 | 生命周期 | 例子 |
|------|------|---------|------|
| **Contract** | 行为契约：接受什么、保证什么 | 最稳定，接口变更时更新 | "接受非空切片，返回排序结果" |
| **Intent** | 设计意图：为什么这样做 | 重大重构时可能更新 | "用归并排序是因为需要稳定排序" |
| **Invariant** | 不变量：什么必须永远成立 | 一旦确立很少变 | "id 字段构造后永远为正数" |
| **Fragility** | 已知风险：什么地方脆弱 | 修复后可能降级或消除 | "浮点比较没有 eps，边界可能出错" |
| **Correction** | 修正记录：上次哪里错了 | append-only，永远保留 | "上次改成 try-catch 是错的，根因是上游没校验" |

不同分类有不同的更新策略和可信度权重。

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

当 Evidence 与 Assertion 矛盾时：

```
1. 标记被证伪的 Assertion 为 Retracted
2. 沿 depends 边 BFS 找到所有下游 Assertion
3. 对每个下游 Assertion：
   - 有其他独立 grounds → 保留，标记 grounds 变更
   - 唯一 grounds 是被证伪者 → 标记为 Uncertain
4. 基于新 Observation 创建修正后的 Assertion
5. 重新推导受影响 Assertion 在新前提下的状态
```

此算法基于 Doyle (1979) 的 **Truth Maintenance System (TMS)**，是经过 40+ 年验证的信念维护方法。

#### 唯一

模型在任意时刻只有一个活跃状态。所有历史保留在 append-only changelog 中，但当前状态是唯一的。不存在分支或多个候选。

---

## 3 对 Coding Agent 的适配

### 3.1 双层架构

Entity 网络分为两层，分离自动提取与人工推理：

```
┌─────────────────────────────────────────────┐
│              自动层（工具链提取）           │
│                                             │
│  文件包含关系    函数调用图    类型依赖     │
│  import/use     trait 实现   公开接口       │
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

自动层提供结构骨架，理解层填充语义内容。LLM 不需要在"函数 A 调用了函数 B"这种事实上浪费推理。

### 3.2 粒度控制

不是每个代码符号都需要 Assertion。粒度规则：

| 层级 | 是否记录 | 判定标准 |
|------|---------|---------|
| 模块职责 | **必定记录** | 模块的存在目的 / 核心使命 |
| 公开接口契约 | **必定记录** | 调用方依赖的行为保证 |
| 关键不变量 | **必定记录** | 违反会导致难以定位的 bug |
| 设计意图 | 选择性记录 | 非显而易见的决策 |
| 已知风险 | 选择性记录 | 已识别但暂未修复的边界 |
| 内部辅助函数 | 通常不记录 | 标准模式、样板代码 |

核心判定标准："如果这个假设被违反，会不会导致难以定位的 bug？"

### 3.3 Agent 工作流中的模型交互

```
┌─────────────────────────────────────────────────────────┐
│                   Coding Agent 工作流                   │
│                                                         │
│  1. 收到任务                                            │
│     └→ 查询模型: 获取涉及 Entity 的认知上下文           │
│                                                         │
│  2. 规划变更                                            │
│     └→ 影响面分析: 改动可能影响哪些 Entity 和 Assertion │
│                                                         │
│  3. 实现变更                                            │
│     └→ 遵守已知 Invariant，不违反 Contract              │
│                                                         │
│  4. 验证变更                                            │
│     └→ 对受影响 Assertion 重新收集 Evidence             │
│     └→ 如有失败 → 进入 5                                │
│                                                         │
│  5. 追溯根因                                            │
│     └→ 沿 depends 链回溯，定位被违反的 Assertion        │
│     └→ 检查每条 Evidence，找到第一个不成立的            │
│                                                         │
│  6. 修正认知                                            │
│     └→ Retract 错误 Assertion + 级联更新                │
│     └→ Assert 修正后的新 Assertion + 记录 Correction    │
│     └→ 修正代码                                         │
│                                                         │
│  7. 持久化                                              │
│     └→ 模型写入磁盘，跨 session 保持                    │
└─────────────────────────────────────────────────────────┘
```

### 3.4 Bootstrap 策略

模型不需要一次性构建，而是随实践渐进积累：

```
Session 1: 模型为空
  → Agent 处理第一个 task
  → 构建涉及的 Entity 和 Assertion
  → 模型有 3 个 Entity，5 条 Assertion

Session 2: 模型有少量内容
  → Agent 处理第二个 task，部分可复用已有模型
  → 新增涉及的 Entity，可能修正已有 Assertion
  → 模型有 8 个 Entity，15 条 Assertion

Week 2: 模型逐渐丰满
  → Agent 对核心模块已有完整认知
  → 新 task 大量复用已有理解
  → 模型开始发挥"认知稳定性"作用
```

这与人类加入新团队的过程一致——认知模型是渐进构建的，而非预装完成的。

---

## 4 工程方案

### 4.1 架构：LLM + 确定性服务

认知模型作为独立服务存在，与 LLM 形成分工：

```
┌───────────────────────────────────────────────────┐
│            Coding Agent (LLM)                     │
│                                                   │
│  职责：语义判断                                   │
│  - 读代码，理解在做什么                           │
│  - 做出决策，写出代码                             │
│  - 遇到 bug 时判断哪个假设错了                    │
│                                                   │
│  通过CLI工具配合SKILL指南与外部认知模型交互       │
└──────────────┬────────────────────────────────────┘
               │
               ▼
┌─────────────────────────────────────────────┐
│          Cognitive Model Service            │
│                                             │
│  职责：结构维护                             │
│  - 图存储与查询 (SQLite)                    │
│  - 依赖追踪 (BFS/DFS)                       │
│  - 级联更新 (retract → cascade)             │
│  - 一致性验证 (verify)                      │
│  - Changelog (append-only audit trail)      │
│  - 增量查询 (按需返回子图，控制 token)      │
└─────────────────────────────────────────────┘
```

**分工原则**：LLM 负责"理解"和"判断"，程序负责"记忆"和"一致性"。

### 4.2 存储方案

采用 SQLite 单文件数据库：

| 选择理由 | 说明 |
|---------|------|
| 图遍历 | SQL 递归查询支持 BFS/DFS |
| 事务保证 | 级联更新不能改一半失败 |
| 索引 | 快速查询，不需全表扫描 |
| Git 友好 | 单文件，可版本化 |
| 零依赖 | 嵌入式，无需独立服务进程 |

### 4.3 CLI 接口设计

#### 查询类

```bash
# 获取实体的认知上下文（assertions + 1-hop 相关实体）
cog query <entity>
# 返回 ≈ 200-500 tokens

# 分析变更影响面（所有下游依赖的 entity 和 assertion）
cog impact <entity>
# 返回 ≈ 100-300 tokens

# 从症状追溯根因（沿 depends 链回溯，检查每条 evidence）
cog trace --entity <entity> --symptom "<description>"
# 返回 ≈ 200-500 tokens

# 列出当前模型中的所有实体（轻量索引）
cog index
# 返回 ≈ 50-200 tokens
```

#### 写入类

```bash
# 记录新的认知断言
cog assert <entity> \
  --kind <Contract|Intent|Invariant|Fragility|Correction> \
  --claim "<claim text>" \
  --grounds "<source>:<detail>" \
  [--depends-on <assertion-id>]

# 撤回错误的认知（自动级联更新下游）
cog retract <assertion-id> --reason "<why it's wrong>"
# 返回受影响的 assertion 列表，供 LLM 复核

# 标记实体间的依赖（补充 LLM 遗漏的结构关系）
cog depend <entity-a> --on <entity-b> --kind <calls|uses|contains>
```

#### 验证类

```bash
# 检查指定范围内所有 Active Assertion 的一致性
cog verify [--scope <entity>]
# 返回 stale / inconsistent assertions

# 导出模型状态（用于调试、review、迁移）
cog export [--format json|toml|dot]

# 模型统计信息
cog stats
# → 实体数 / 断言数 / 修正历史数 / 覆盖率
```

### 4.4 Token 效率分析

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

KV cache 方面：模型查询结果是工具调用的返回值，在同一会话中是稳定的。只要查询接口的输出格式一致，前序查询结果的 KV cache 不会被无效化。

### 4.5 一致性保障

#### 级联更新算法

```
retract(assertion_id, reason):
    assertion = get(assertion_id)
    assertion.status = Retracted(reason)

    # BFS 沿 depends_on 边找到所有受影响的下游 Assertion
    queue = [assertion_id]
    affected = []

    while queue is not empty:
        current = queue.dequeue()
        dependents = find_dependents(current)  # 反向边查询

        for dep in dependents:
            if dep has_other_independent_grounds(current):
                dep.add_note("ground weakened: {current} retracted")
            else:
                dep.status = Uncertain
                queue.enqueue(dep.id)
            affected.append(dep)

    changelog.append(Retracted(assertion_id, reason, affected))
    return affected  # 返回给 LLM 复核
```

关键点：级联遍历是确定性图算法，不依赖 LLM。

#### 降低依赖遗漏风险的机制

| 机制 | 原理 |
|------|------|
| 自动结构依赖 | Entity 间的调用/包含关系由工具链提取，不经 LLM |
| Conservative default | 不确定时宁可标 depend，过度连接比遗漏安全 |
| 影响面预检 | retract 前先返回下游清单，LLM 复核是否有遗漏 |
| 周期性验证 | `cog verify` 重新检查所有 Active Assertion 的 grounds |

### 4.6 Agent 框架集成

在 agent 框架中，模型 CLI 注册为一组工具：

```
tools = [
    Tool("cog_query",   "查询实体的认知模型上下文"),
    Tool("cog_impact",  "分析变更影响面"),
    Tool("cog_trace",   "从症状追溯根因"),
    Tool("cog_assert",  "记录新的认知断言"),
    Tool("cog_retract", "撤回错误的认知（自动级联）"),
    Tool("cog_verify",  "验证认知模型一致性"),
]
```

LLM 不需要知道模型内部结构。它只需要在工作流的各个阶段调用对应工具。

---

## 5 与传统方案的对比

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

  → cog trace --entity auth::login --symptom "成功认证但 Token 为空"
  → 返回依赖链:
    Assertion("token 字段构造后为非空字符串")
    → grounds: code:AuthToken
    → 但实际: 并发下 Token::new 可能返回空 token

  → cog retract <token_nonempty_id> --reason "并发竞争导致 Token 可能为空"

  → 自动级联: 标记依赖 "token 字段构造后为非空字符串" 的下游为 Uncertain

  → cog assert AuthToken --kind Correction \
      --claim "Token::new 需要加锁或使用原子操作，并发下可能生成空 token"
      --grounds "runtime_error:并发测试发现空 token"
      --depends-on <retracted_id>

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

## 7 开放问题与后续方向

### 7.1 当前设计中待验证的假设

| 假设 | 风险 | 验证方式 |
|------|------|---------|
| LLM 能可靠地提取 Assertion | LLM 可能生成模糊或错误的断言 | 实际 task 实验，衡量 assertion 准确率 |
| 依赖图足以捕获隐式耦合 | 某些依赖可能是隐式的（如性能特征） | 对比 LLM 标注的 depends_on 与实际耦合 |
| 查询子图足够指导决策 | 可能需要多跳才能覆盖所有相关信息 | 衡量 task 完成质量与查询深度的关系 |
| 渐进 bootstrap 足够有效 | 早期 task 可能因模型不完整而质量下降 | 对比有/无模型的 task 完成质量 |

### 7.2 后续方向

- **自动层集成**：从 LSP / rust-analyzer / cargo metadata 自动提取 Entity 和结构关系
- **Assertion 质量评估**：设计度量标准衡量断言的信息量和准确性
- **跨项目迁移**：相似项目间的认知模型是否可以迁移
- **可视化**：将认知模型渲染为可交互的依赖图，辅助人类 review
- **与 git 集成**：代码变更时自动触发受影响 Assertion 的重新验证

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
