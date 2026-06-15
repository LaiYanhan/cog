# 架构总览

> 本文是代码架构的入口。先看整体分层与依赖方向，再按需进入 [各层文档](#各层文档)。本系统从早期的"Java in Rust"反模式重构为地道 Rust 的六层架构——重构动机见 [decisions/](../decisions/README.md)。

## 六层架构

```
┌─────────────────────────────────────────────────────────────┐
│  第 6 层  实验层 (Experiment Layer) — src/experiment/       │
│  Experiment, ExperimentOp, ExperimentReport                 │
│  "如果假设 X 成立，会发生什么？"                            │
├─────────────────────────────────────────────────────────────┤
│  第 5 层  认知潜空间 (Cognitive Latent Space) — src/space/  │
│  ┌──────────────────────┐ ┌──────────────────────────────┐  │
│  │ 结构子空间 (自动)    │ │ 语义子空间 (TMS 信念系统)    │  │
│  │ StructureSpace       │ │ SemanticSpace                │  │
│  │ 骨架 + 低层关系      │ │ 契约/意图/不变量/风险/修正   │  │
│  └──────────────────────┘ └──────────────────────────────┘  │
│  + CascadeEngine / ImpactEngine / TraceEngine               │
├─────────────────────────────────────────────────────────────┤
│  第 4 层  工作流引导 (Workflow Guide) — src/workflow/       │
│  WorkflowState: Uninit → Ready{phase}                       │
│  + 建议引擎（cog next）                                     │
├───────────────────────────┬─────────────────────────────────┤
│  第 3 层  自动解析        │  第 3 层  手动操作              │
│  src/analysis/            │  src/command/（16 个命令）      │
│  Scanner, ParserPool,     │  assert, retract, depend,       │
│  FileWalker, Extractors   │  query, impact, trace, ...      │
│  确定性、零 LLM           │  agent 驱动的认知构建           │
├───────────────────────────┴─────────────────────────────────┤
│  第 2 层  持久化抽象 (Persistence) — src/repo/              │
│  Repository trait, SqliteRepository                         │
├─────────────────────────────────────────────────────────────┤
│  第 1 层  代码空间 (Code Space) — 被建模的外部世界          │
│  源代码文件、目录结构、tree-sitter 语法树                   │
└─────────────────────────────────────────────────────────────┘
```

每层只依赖直接在它下面的层（编译期通过模块可见性强制）。

## 模块地图

```
src/
├── main.rs              # 入口：DB 定位 + Cli::parse + 分发
├── cli.rs + cli/        # Clap CLI 定义（args/experiment/backup 子模块）
├── domain.rs + domain/  # 领域核心类型（见 02）
├── repo.rs + repo/      # 持久化（见 03）
├── analysis.rs + analysis/  # 自动解析（见 04）
├── command.rs + command/    # 16 个命令（手动操作）
├── space.rs + space/        # 认知潜空间（见 05）
├── workflow.rs + workflow/  # 工作流状态机 + 建议引擎（见 06）
├── experiment.rs + experiment/  # 实验层（见 07）
├── backup.rs + backup/      # 全量备份
└── format.rs + format/      # 输出格式化（见 08）
```

每个 `*.rs` 顶层文件是新式模块声明文件（`mod xxx;`），实际实现移入 `xxx/` 目录——这是 2026-06 的模块迁移结果，目的是把长文件拆成聚焦子模块。

## 依赖方向

```
main ──→ cli ──→ command ──→ space ──→ repo ──→ domain
        │         │
        │         ├──→ workflow （状态文件读写）
        │         ├──→ analysis ──→ repo ──→ domain
        │         ├──→ experiment ──→ space ──→ repo ──→ domain
        │         └──→ backup ──→ repo
        │
        └──→ format
```

`cli` 编排 `command`、`workflow`、`analysis`、`experiment`、`backup`。各子系统之间不互相依赖——它们都通过 `domain`（共享类型）和 `repo`（持久化）间接协作。

## 入口与分发

`src/main.rs` 做三件事：

1. **定位 DB**：优先 `cog sync --init` 路径；否则从 CWD 向上查找 `.cog/cog.db`；否则用显式 `--db`；都没有则报错引导 `cog sync --init`。
2. **打开仓库**：`SqliteRepository::open(&db_path)`（应用 schema + 迁移）。
3. **执行**：`cli.run(&store)` 加载 `WorkflowState`、分发命令、应用状态转换、保存状态、emit 输出。

`src/cli.rs` 的 `Cli::run` 是分发核心（见 [reference/01-cli-reference.md](../reference/01-cli-reference.md) 的命令清单）。每个命令返回 `CommandOutput { text, exit_code, has_drift }`，其中 `has_drift` 是 `sync` 专属字段，驱动 WorkflowState 转换而不必解析输出文本。

## 各层文档

| 文档 | 层 |
|------|----|
| [02-domain-layer.md](02-domain-layer.md) | 领域类型（Entity/Assertion/Evidence/Relations/Naming） |
| [03-persistence-layer.md](03-persistence-layer.md) | Repository trait + SqliteRepository + schema |
| [04-analysis-layer.md](04-analysis-layer.md) | Scanner/ParserPool/FileWalker/Extractors |
| [05-space-layer.md](05-space-layer.md) | StructureSpace/SemanticSpace/Cascade/Impact/Trace |
| [06-workflow-layer.md](06-workflow-layer.md) | WorkflowState + 建议引擎 |
| [07-experiment-layer.md](07-experiment-layer.md) | Experiment 生命周期 |
| [08-output-layer.md](08-output-layer.md) | Renderable/TextRenderer/CommandOutput |

## 设计原则

五条从 Rust 社区共识提炼、指导本架构的原则：

1. **数据为王，行为跟随**——领域类型只做纯计算，副作用通过 `Repository` trait 在 command 层完成。
2. **模块是封装边界，不是文件夹**——每个模块暴露 3-5 个公共类型，其余 `pub(crate)` 内部化。
3. **Trait 只为真正的多态而存在**——`Repository` 是唯一需要 trait 抽象的层（变更隔离 + 可测试性）。
4. **Typestate 编码进程内业务规则**——仅适用于进程内对象（如 Experiment 的状态检查），不适用于无状态的 CLI 工作流（每次调用是新进程）。
5. **组合优于分类**——Rust 无继承，组合是唯一复用方式。

## 数据流（每个命令）

```
cli.rs → Cli::run(&store)
       → WorkflowState::load(cog_dir)
       → command::<module>::execute(&dyn Repository, args) → CommandOutput
       → wf.transition_*()  # 隐式状态转换
       → wf.save(cog_dir)
       → output.emit()
```

例外：`sync` 非空跑时包裹在 `store.transaction()` 中以保证原子性；`experiment commit` 后将 phase 设为 `PendingImplement`。
