use tree_sitter::{Node, TreeCursor};

use super::{Call, Definition, Import, node_text};
use crate::domain::EntityKind;

pub fn extract_go<'a>(
    root: &Node<'a>,
    source: &str,
    module_qname: &str,
    cursor: &mut TreeCursor<'a>,
) -> (Vec<Definition>, Vec<Import>, Vec<Call>) {
    let mut defs = Vec::new();
    let mut imports = Vec::new();
    let mut calls = Vec::new();

    for child in root.children(cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    let fqname = format!("{module_qname}::{name}");
                    defs.push(Definition {
                        qualified_name: fqname.clone(),
                        kind: EntityKind::Function,
                        parent: None,
                    });
                    extract_calls_from_block(&child, source, &fqname, &mut calls);
                }
            }
            "method_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let method_name = node_text(&name_node, source);
                    let receiver = child.child_by_field_name("receiver").and_then(|r| {
                        let mut cur = r.walk();
                        r.children(&mut cur).find_map(|n| {
                            if n.kind() == "type_identifier" {
                                Some(node_text(&n, source))
                            } else if n.kind() == "pointer_type" {
                                let mut ic = n.walk();
                                n.children(&mut ic).find_map(|c| {
                                    if c.kind() == "type_identifier" {
                                        Some(node_text(&c, source))
                                    } else {
                                        None
                                    }
                                })
                            } else {
                                None
                            }
                        })
                    });
                    let fqname = if let Some(recv) = &receiver {
                        format!("{module_qname}::{recv}::{method_name}")
                    } else {
                        format!("{module_qname}::{method_name}")
                    };
                    defs.push(Definition {
                        qualified_name: fqname.clone(),
                        kind: EntityKind::Method,
                        parent: receiver,
                    });
                    extract_calls_from_block(&child, source, &fqname, &mut calls);
                }
            }
            "type_declaration" => {
                let mut cur = child.walk();
                for type_child in child.children(&mut cur) {
                    if type_child.kind() == "type_spec"
                        && let Some(name_node) = type_child.child_by_field_name("name")
                    {
                        let name = node_text(&name_node, source);
                        defs.push(Definition {
                            qualified_name: format!("{module_qname}::{name}"),
                            kind: EntityKind::Type,
                            parent: None,
                        });
                    }
                }
            }
            "import_declaration" => {
                extract_go_import(&child, source, &mut imports);
            }
            _ => {}
        }
    }

    (defs, imports, calls)
}

/// Walk a function body looking for `call_expression` nodes.
fn extract_calls_from_block(
    func_node: &Node,
    source: &str,
    caller_qname: &str,
    calls: &mut Vec<Call>,
) {
    let mut cursor = func_node.walk();
    for child in func_node.children(&mut cursor) {
        if child.kind() == "block" {
            walk_for_calls(&child, source, caller_qname, calls);
            break;
        }
    }
}

/// Recursively walk an AST subtree looking for `call_expression` nodes.
fn walk_for_calls(node: &Node, source: &str, caller_qname: &str, calls: &mut Vec<Call>) {
    if node.kind() == "call_expression"
        && let Some(func) = node.child_by_field_name("function")
    {
        let callee = extract_callee_name(&func, source);
        if !callee.is_empty() {
            calls.push(Call {
                callee_name: callee,
                caller_qname: caller_qname.to_string(),
            });
        }
    }
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        walk_for_calls(&child, source, caller_qname, calls);
    }
}

/// Extract the simple function/method name from a Go call expression callee.
/// Handles `foo()`, `obj.Method()`, `pkg.Function()`.
fn extract_callee_name(func_node: &Node, source: &str) -> String {
    match func_node.kind() {
        "identifier" => node_text(func_node, source),
        "selector_expression" => func_node
            .child_by_field_name("field")
            .map(|n| node_text(&n, source))
            .unwrap_or_default(),
        "parenthesized_expression" => String::new(),
        _ => String::new(),
    }
}

fn extract_go_import(node: &Node, source: &str, imports: &mut Vec<Import>) {
    // When called directly with an import_spec (from import_spec_list recursion),
    // process this node's own children to extract module path and alias.
    if node.kind() == "import_spec" {
        let mut module = String::new();
        let mut alias = String::new();
        let mut spec_cursor = node.walk();
        for spec_child in node.children(&mut spec_cursor) {
            match spec_child.kind() {
                "interpreted_string_literal" => {
                    module = node_text(&spec_child, source).trim_matches('"').to_string();
                }
                "package_identifier" | "blank_identifier" | "dot" => {
                    alias = node_text(&spec_child, source);
                }
                _ => {}
            }
        }
        if !module.is_empty() {
            imports.push(Import {
                module_path: module,
                imported_names: if alias.is_empty() || alias == "_" || alias == "." {
                    Vec::new()
                } else {
                    vec![alias]
                },
            });
        }
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_spec" => {
                extract_go_import(&child, source, imports);
            }
            "import_spec_list" => {
                let mut list_cursor = child.walk();
                for spec in child.children(&mut list_cursor) {
                    if spec.kind() == "import_spec" {
                        extract_go_import(&spec, source, imports);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    /// Parse Go source and call extract_go_import on the import_declaration
    /// node directly.
    fn extract_imports(source: &str) -> Vec<Import> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .expect("failed to set Go language");
        let tree = parser
            .parse(source, None)
            .expect("failed to parse Go source");
        let root = tree.root_node();
        let mut cursor = root.walk();
        let mut imports = Vec::new();
        for child in root.children(&mut cursor) {
            if child.kind() == "import_declaration" {
                extract_go_import(&child, source, &mut imports);
            }
        }
        imports
    }

    #[test]
    fn single_import_no_alias() {
        let imports = extract_imports(
            r#"package main
import "fmt"
func main() { fmt.Println("hello") }
"#,
        );
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "fmt");
        assert!(imports[0].imported_names.is_empty());
    }

    #[test]
    fn single_import_with_alias() {
        let imports = extract_imports(
            r#"package main
import f "fmt"
func main() { f.Println("hello") }
"#,
        );
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "fmt");
        assert_eq!(imports[0].imported_names, vec!["f"]);
    }

    #[test]
    fn blank_import() {
        let imports = extract_imports(
            r#"package main
import _ "net/http/pprof"
func main() {}
"#,
        );
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "net/http/pprof");
        assert!(imports[0].imported_names.is_empty());
    }

    #[test]
    fn dot_import() {
        let imports = extract_imports(
            r#"package main
import . "math"
func main() {}
"#,
        );
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].module_path, "math");
        assert!(imports[0].imported_names.is_empty());
    }

    #[test]
    fn multi_import_block_with_mix() {
        let imports = extract_imports(
            r#"package main
import (
    "fmt"
    f "fmt"
    _ "net/http/pprof"
    . "math"
)
func main() {}
"#,
        );
        assert_eq!(
            imports.len(),
            4,
            "expected 4 imports, got {}: {:#?}",
            imports.len(),
            imports
        );
        assert_eq!(imports[0].module_path, "fmt");
        assert!(imports[0].imported_names.is_empty());
        assert_eq!(imports[1].module_path, "fmt");
        assert_eq!(imports[1].imported_names, vec!["f"]);
        assert_eq!(imports[2].module_path, "net/http/pprof");
        assert!(imports[2].imported_names.is_empty());
        assert_eq!(imports[3].module_path, "math");
        assert!(imports[3].imported_names.is_empty());
    }
}
