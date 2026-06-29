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
