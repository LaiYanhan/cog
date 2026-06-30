# TODO — 已知问题（暂缓，按需再启动）

> 源自 2026-06-29 的 cog 自举实测（`/tmp/minilang`）。这两条经评估为「低收益 / 可延后」，
> 故先记录在此，不并入当前修复批次。触发条件满足时再启动。

## #6 手动实体无法覆盖 EntityKind

**现象**：`EntityKind::infer`(`src/domain/entity.rs:96`) 把「带 `::` 的小写名」一律判为 Function，
导致设计期手动建的模块路径（如 `minilang::ast`、`minilang::lexer`）被误判为 Function。
**注意**：scan 产出的模块（`src::interp` 等）由 tree-sitter 提取器直接给定正确 kind，
**只有 Manual 实体**受此启发式限制。

**为何暂缓**：Manual 实体是过渡态（设计期 → 实现 grounding 后通常被 Scan 实体取代或 migrate 走）；
误判 kind 对结构推理影响有限，收益低于工时。

**升级路径**：加 `cog set-kind <entity> <module|function|type|method|field>` 命令，
或给 `assert` 增加 entity-kind 覆盖 flag（注意与断言 `--kind` 区分命名）。
落地时同步更新 `docs/reference/01-cli-reference.md` 与 `skills/cog/SKILL.md`。

## #13 `line_count` 对 Rust 恒为 null

**现象**：`AGENTS.md` / `docs/skills/WORKFLOWS.md` 承诺 metrics 含 `line_count`，
但 Rust 提取器未捕获行范围，所有 Rust 实体 `line_count` 均为 `null`。

**为何暂缓**：tree-sitter 节点已带 `start_position`/`end_position`，捕获不难，
但收益主要在排序/展示，不影响依赖推理与级联。优先级低于 P0/P1。

**升级路径**：在 `src/analysis/extractors.rs` 的 Rust 提取器里用节点行范围计算 `line_count`，
随 `Definition` 一并写入；其它语言（Python/Go/...）同步补齐。
落地时补一条单测断言行数非 null。

---

## usage 模块的延后项（2026-06-30）

> 源自本次 usage 日志/统计功能的实现。核心已完成并验证：`cog usage` 默认合并 usage.jsonl
> （命令调用）与 changelog（模型变更，按 `action` 聚合，零 detail 解析）。以下为评估后暂缓的增强项。

## #14 changelog.detail 为自由文本，阻塞深度下钻

**现象**：标题级聚合（按 `action` 计数）已工作，但 `changelog.detail` 是自由文本
（如 sync 的 `"created=465 removed=0 relations=1023"`），无法做按实体 / 按 kind / 按
时间的结构化下钻。

**为何暂缓**：当前研究问题（"认知层是否被用"）只需 action 级计数即可回答；`detail`
文本只影响深度下钻维度。收益低于工时。

**升级路径**：将 `detail` 改为 JSON（或加性新增 `detail_json` 列），更新全部
`append_changelog` 调用点（约 10 处：assert/retract/cascade_mark/depend/sync/verify/
delete_entity/migrate）。需容忍/迁移旧文本行。落地后 `cog usage` 可暴露下钻维度。

## #15 usage.jsonl 与 changelog 未做时间轴联合

**现象**：`cog usage` 把两路 feed 并列展示（"By command" + "By mutation"），但未按
timestamp join 成统一时间线（如"该 session 第 3 条命令是 assert，对应 changelog 第 5 条"）。

**为何暂缓**：并列视图已回答频率/密度问题；统一时间线服务于行为序列分析
（agent 在一个 session 内的读写节奏），属更深分析层。

**升级路径**：在 `src/usage/analyze.rs` 增加按 timestamp 归并的 unified timeline，
`cog usage --timeline` 输出，或导出供外部工具。

## #16 `cog usage` 缺时间窗过滤与 CSV 导出

**现象**：设计阶段提出 `--since <duration>` 与 CSV 导出，实现时只做了 `--raw` + 全局
`--output text|json`。

**为何暂缓**：当前数据量小，全量查看足够；CSV 可由 `--output json` + `jq`/pandas 替代。

**升级路径**：加 `--since <7d|24h|...>` 过滤；加 `--output csv`（注意 OutputFormat 枚举扩展）。

## #17 verify 的读/写归类偏保守

**现象**：`src/usage/analyze.rs::is_read` 把 verify 归为"写"（因 `--clean` 会删孤立实体），
但 verify 多为只读检查，导致 reads 计数偏低。

**为何暂缓**：`by_command` 明细里有精确 verify 计数，reads/writes 只是粗略启发式。

**升级路径**：按 `--clean` 是否存在动态归类（usage 事件 args 已记录 `clean` flag）。

## #18 概率化 TMS（continuous belief）—— 取决于实测数据

**背景**：曾设计把 Doyle 三值 TMS 升级为连续信念（Beta 分布 + noisy-OR 沿 `depends_on`
传播），并用 Python 原型验证了可行性与确定性（DAG 上纯函数，无 RNG）。

**为何暂缓**：该升级的前提是 `depends_on` 在真实使用中有足够密度。usage 实测尚未进行；
数据出来前升级是空中楼阁。

**升级路径**：先用 `cog usage` 跑真实任务（如 Lua 编译器），观察 `depend` / `cascade_mark`
密度。密度足够 → 实施概率化 TMS（见 `src/space/cascade.rs`）；若 `depends_on` 始终为 0 →
改走"断言依赖自动流过 calls 图"路线，而非升级离散 TMS。
