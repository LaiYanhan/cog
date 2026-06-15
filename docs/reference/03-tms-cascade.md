# TMS 级联算法

> Truth Maintenance System 级联的精确语义。这是 cog 的核心机制，也是最容易在文档中被误述的部分。源自 `src/space/cascade.rs` + `src/space/semantic.rs`。

## 存储状态 vs 报告原因（关键澄清）

最常见的误解是把 `GroundWeakened` 当作存储状态。**事实**：

- **存储的 `AssertionStatus` 只有 3 种**：`Active`、`Retracted`、`Uncertain`。
- **`CascadeReason` 有 2 种**（仅在级联报告中出现）：`MarkedUncertain`、`GroundWeakened`。

| CascadeReason | 含义 | 存储状态变更 |
|---------------|------|-------------|
| `GroundWeakened` | 该 dependent 还有其它独立 Active 依赖 | **无**——保持 `Active` |
| `MarkedUncertain` | 该 dependent 的所有依赖都非 Active | **改为 `Uncertain`** + changelog |

`assertions.status` 的 DB CHECK 约束（`IN ('active','retracted','uncertain')`）从数据库层面强制了这一点。

## 算法：`CascadeEngine::retract`

```
retract(assertion_id, reason):
  current = get_assertion(assertion_id)         # 不存在则 bail
  if current.status == Retracted: bail("already retracted")

  Phase 1: SemanticSpace::load(repo, current.entity_id)   # 验证上下文

  retract_assertion(assertion_id, reason)       # 标记 Retracted
  append_changelog(Retract, assertion_id, reason)

  Phase 2: affected = apply_cascade(repo, assertion_id)   # BFS 真实执行

  return CascadeReport { retracted: current, affected }
```

## BFS 级联：`apply_cascade`

```
queue = [retracted_id]
seen = {}
affected = []

while queue:
  current_id = queue.dequeue()
  if current_id in seen: continue
  seen.add(current_id)

  for dependent in get_dependents(current_id):       # 反向 depends_on 边
    if dependent.status == Retracted: continue

    dependencies = get_dependencies(dependent.id)
    has_independent_active = dependencies.any(dep =>
        dep.id != current_id
        && dep.status != Retracted
        && dep.status != Uncertain)

    if has_independent_active:
      affected += { dependent, GroundWeakened }     # 仍 active，只报告
      continue                                       # 不入队——不继续传播

    if dependent.status != Uncertain:
      update_assertion_status(dependent.id, Uncertain)
    append_changelog(CascadeMark, dependent.id,
        "marked uncertain due to dependency retraction: {current_id}")

    queue.enqueue(dependent.id)
    affected += { dependent(now Uncertain), MarkedUncertain }

return affected
```

## 为何用真实 BFS 而非模拟结果

`SemanticSpace::simulate_retract` 也能算级联，但实验（experiment）的 `evaluate()` 才用它。CLI 的 `retract` 用 `apply_cascade` 在**真实 DB** 上逐节点 BFS，原因是：

**"是否有独立 Active 依赖"的检查需要实时全局状态**。`simulate_retract` 只知道加载到子空间的边——子图不完整，可能漏算某个未加载的 Active 依赖，导致误判为 Uncertain。`apply_cascade` 每步实时 `get_dependencies`，查的是当前 DB 全量状态。

这是"模拟与执行分离"的体现——experiment 用模拟（轻量、可丢弃），CLI `retract` 用执行（真实、需准确）。

## 恢复机制：`cog recover`

`Uncertain` 不是终态。当一个 `Uncertain` 断言的所有 `dependencies` 重新变为 `Active` 时（例如 agent 用 `assert --replace` 修正了被 retract 的依赖），它可以恢复：

```
recover(--apply):
  uncertain = list_assertions().filter(status == Uncertain)
  for a in uncertain:
    deps = get_dependencies(a.id)
    if deps.all(d => d.status == Active):
      recoverable += a
    else:
      blocked += (a, count of inactive deps)
  if --apply: update_assertion_status(recoverable, Active)
```

`recover` 不修改 `blocked` 的断言。注意：级联只把"唯一支撑断裂"的标记为 Uncertain——所以恢复只需检查依赖是否全部 Active。

## 触发与影响

| 操作 | 是否级联 |
|------|---------|
| `retract <id>` | 是——`CascadeEngine::retract` |
| `experiment commit`（含 Retraction op） | 是——commit 内调 `CascadeEngine::retract` |
| `experiment evaluate`（含 Retraction op） | 模拟——`SemanticSpace::simulate_retract`，不触 DB |
| `delete-entity` | 否——级联删除是 DB 外键级联，不走 TMS |

## 设计要点

- 级联是**确定性图算法**，不依赖 LLM——把"知识变动的影响评估"变成 O(V+E) 的 BFS。
- 二态模型（GroundWeakened 报告 / MarkedUncertain 存储）在实践中已足够——无论哪种，agent 下一步都是"检查受影响断言，决定修正或确认"。
- `retract` 命令现在接受 `&dyn Repository`（级联逐条提交），不再需要 `&SqliteRepository` 的事务包裹。
