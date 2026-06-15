# 第 3 层（自动解析）：Analysis Pipeline — `src/analysis/`

> tree-sitter 驱动的代码空间扫描器。确定性、零 LLM 开销、幂等。它**不接触** `Repository`——只返回纯数据 `ScanReport`，由 `sync` 命令负责写入。

## 结构

```
src/analysis/
├── scanner.rs       # Scanner：遍历 + 调度 + scan_file
├── walker.rs        # FileWalker：gitignore 感知的 BFS 文件遍历
├── pool.rs          # ParserPool：按语言缓存 tree-sitter Parser
├── report.rs        # ScanReport：纯数据扫描结果
├── languages.rs     # Language enum（Rust/Python/JS/Go/C/Java）
├── extractors.rs    # 共享提取辅助 + CallSite 类型
└── extractors/      # 每语言一个提取器
    ├── rust.rs, python.rs, javascript.rs, go.rs, c.rs, java.rs
```

## 核心类型

```rust
pub struct Scanner { pool: ParserPool }

pub struct ScanConfig { root: PathBuf, languages: Option<Vec<Language>> }

pub struct ScanReport {
    pub file_scans: Vec<FileScan>,      // 每文件的 definitions + imports + calls
    pub definitions: Vec<Definition>,   // 全部定义（聚合）
    pub imports: Vec<Import>,           // 全部导入（聚合）
    pub languages_detected: Vec<Language>,
}
```

`FileScan` 含 `path`、`definitions`、`imports`、`calls`（调用点，用于建立 `Calls` 关系）。

## 扫描流程

`Scanner::scan(config)`：

1. `FileWalker` 从 `root` 做 BFS，跳过隐藏目录、`target/`、`node_modules/`、`__pycache__/` 等。使用 `ignore` crate，**gitignore 感知**——`.gitignore` 中的路径自动跳过。
2. 按扩展名识别 `Language`（`--lang` 可过滤）。
3. 对每个文件 `scan_file(path, lang, root, pool)`：用 `ParserPool::acquire(lang)` 取缓存的 parser → tree-sitter 解析 → 调用对应语言的 extractor 提取 definitions/imports/calls。
4. 聚合成 `ScanReport` 返回。

## ParserPool

按语言缓存 `tree_sitter::Parser`，避免每个文件 `Parser::new()`。`acquire(lang)` 返回 `&mut Parser`。

## 语言提取器

每个提取器是自由函数，匹配 tree-sitter 的 node kinds：

| 语言 | 定义 node kinds | 导入 node kinds |
|------|----------------|----------------|
| Rust | `function_item`、`struct_item`、`enum_item`、`trait_item`、`impl_item` | `use_declaration` |
| Python | `function_definition`、`class_definition` | `import_statement`、`import_from_statement` |
| JavaScript | `function_declaration`、`generator_function_declaration`、`class_declaration`、`export_statement` | `import_statement` |
| Go | `function_declaration`、`method_declaration`、`type_declaration` | `import_declaration` |
| C | `function_definition`、`struct_specifier`、`type_definition` | `preproc_include` |
| Java | `class_declaration`、`method_declaration` | `import_declaration` |

调用点（`calls`）从源码中提取 callee 简单名 + caller qname，供 `sync` 建立 `Calls` 关系（按简单名匹配定义实体）。

`node_text(node, source)` 辅助提取节点源文本。

## qualified name 推导

`qualified_name_from_path(path, root, lang)`：`src/auth/login.rs` 相对 root → `auth::login`（剥离 `src/`/`lib/`/`pkg/` 前缀 + 扩展名，`/` → `::`）。Python 剥 `.py`。

## 与 sync 的契约

`analysis` 只产出数据。`command::sync_cmd` 消费 `ScanReport`：

- 为每个 definition 创建/更新 `Scan` origin entity。
- 创建目录/文件 Module entity 与 `Contains` 层级。
- 由 imports 创建 `Uses` 关系，由 calls 创建 `Calls` 关系。
- 计算并持久化 `fan_in`/`fan_out` metrics。
- 删除不再存在的 stale `Scan` entity（保护有 assertion 的）。

详见 sync 命令实现（`src/command/sync_cmd.rs`）与 [reference/01-cli-reference.md](../reference/01-cli-reference.md)。

## 设计约束

- **零数据库依赖**：`Scanner` 不持有 `Repository`，返回纯 `ScanReport`。这让扫描可独立测试、可 dry-run。
- 确定性：相同代码 + 相同 `Language` 集合 → 相同 `ScanReport`（无随机、无 LLM）。
- 新增语言 = 新增 `extractors/<lang>.rs` + `Language` 变体 + 扩展名识别。
