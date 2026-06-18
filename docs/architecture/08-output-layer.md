# 输出格式化层 — `src/format/` + `CommandOutput`

> 横切关注点：把领域报告类型路由到人类可读文本或机器可读 JSON。所有命令通过这一层产出输出，不自己拼字符串。

## 结构

```
src/format/
├── (format.rs)    # OutputFormat enum + Renderable trait + emit_report()
├── text.rs        # 模块声明，re-export text 子模块
└── text/
    ├── renderable.rs  # 各报告类型的 Renderable 实现
    └── reports.rs     # TextRenderer 方法（人类可读渲染）
```

## 核心抽象

```rust
pub enum OutputFormat { Text, Json }

pub trait Renderable {
    fn render_text(&self) -> String;
}

pub fn emit_report<T: serde::Serialize + Renderable>(report: &T, format: OutputFormat) -> String
```

每个命令构建领域报告类型（定义在 `domain/report.rs`），同时 derive `Serialize` + 实现 `Renderable`，再 `emit_report(&report, self.output)` 路由：

- `Text` → `report.render_text()`（`TextRenderer`）
- `Json` → `serde_json::to_string_pretty(report)`（内联于 `emit_report`）

## CommandOutput

```rust
pub struct CommandOutput {
    pub text: String,
    pub exit_code: i32,
    pub has_drift: bool,   // sync 专属：是否检测到代码 drift
}
```

`has_drift` 是关键设计：`sync` 根据 `SyncReport.has_drift` 设置它，`cli.rs` 直接读 `out.has_drift` 驱动 `transition_sync`——**不解析格式化文本**推断状态。这避免了"输出文本格式变化导致状态转换悄悄失效"的风险。

构造：`CommandOutput::success(text)`（exit 0，no drift）、`with_exit_code(text, code)`。`emit()` 打印到 stdout。

## TextRenderer

`text/reports.rs`（~31KB）是最大的格式化文件，为每种报告类型提供渲染方法：`cascade_report`、`query_card`、`impact_card`、`sync_report`、`next_report` 等。`text/renderable.rs` 是各类型的 `Renderable::render_text()` 薄封装。

`domain/display.rs` 提供跨命令共享的展示辅助（`AssertedEntity`、`partition_by_assertion`、`entities_word`、`plural_s`），避免格式化逻辑重复。

## JSON 输出

`emit_report` 的 JSON 分支直接调用 `serde_json::to_string_pretty`。由于报告类型 derive `Serialize` 且字段用 `#[serde(skip_serializing_if)]` 等控制，JSON 输出忠实反映结构。`--output json` 是全局 flag。

## 设计约束

- **格式与领域逻辑解耦**：命令只构建报告类型，不拼字符串。这是与早期"工具类 Formatter"反模式的区别——加 `--output json` 是加法，不改动命令。
- 不用 `Display` trait——它会锁死单一输出格式。`Renderable` + `TextRenderer`（文本）+ `serde_json`（JSON 内联于 `emit_report`）让多输出格式可扩展。
- `has_drift` 这类"带外信号"通过结构体字段传递，不通过文本解析。
