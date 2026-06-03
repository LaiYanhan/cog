# cog TODO

## 推理介质改进（核心方向）

### 方向一：丰富依赖关系的语义
- [ ] 设计 `refines` 关系类型：A 是对 B 的细化，B retract 时 A 变为 GroundWeakened 而非 Uncertain
- [ ] 设计 `contradicts` 关系类型：A assert 时自动将矛盾的 B 标记为 Uncertain
- [ ] 设计 `supersedes` 关系类型：A 替代 B，B retract 对 A 无影响
- [ ] 扩展 `--depends-on` 参数接受关系类型：`--depends-on <id>:refines`
- [ ] 修改 cascade 算法（`graph.rs`）根据关系类型选择级联策略
- [ ] 更新 `retract` 和 `add_assertion_dependency` 的 CLI 接口
- [ ] 更新 format 输出以展示关系类型

### 方向二：将分支进化为推理沙箱
- [ ] 支持在分支中执行 `impact` 命令，对比主模型与分支模型的差异
- [ ] 支持在分支中执行 `verify`，检查假设性知识与主模型是否矛盾
- [ ] `branch diff` 输出增加受影响的断言数量和下游影响范围摘要
- [ ] 设计 `branch experiment` 子命令：创建分支 → 断言假设 → 自动 impact/verify → 输出风险评估

### 方向三：从"查实体"进化到"问问题"
- [ ] 实现 `cog plan <entity>`：综合 impact 范围 + fragility 密度 + 下游断言状态，输出"改 X 安全吗"的评估
- [ ] 实现 `cog history <entity>`：过滤 correction assertions + changelog，输出实体的事件时间线
- [ ] 实现 `cog path <entity-a> <entity-b>`：双向 BFS 搜索两个实体间的最短路径
- [ ] 实现 `cog changes [--since <date>]`：时间范围内的断言变更摘要（新增、撤销、纠正）

## 实体元数据丰富化（轻度增强）

- [ ] tree-sitter 扫描时提取 `line_count`（节点范围差）存入 entities 表
- [ ] tree-sitter 扫描时提取 `visibility`（pub/private/crate）存入 entities 表
- [ ] 扫描后计算 `fan_in`（入边计数）和 `fan_out`（出边计数）存入 entities 表
- [ ] `cog query` 输出展示这些度量
- [ ] `cog index` 支持按 line_count / fan_in / fan_out 排序
- [ ] schema migration：entities 表增加 line_count、visibility、fan_in、fan_out 列

## 数据模型注意事项

- 所有 schema 变更必须是增量的（只加列/表，不删不改）
- 迁移前必须 `branch create --name pre-migration` 快照
- UUID 稳定性不可破坏
