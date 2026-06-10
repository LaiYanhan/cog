use tree_sitter::{Node, TreeCursor};

use super::{Call, Definition, Import, node_text};
use crate::domain::EntityKind;

pub fn extract_python<'a>(
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
            "function_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    let fqname = format!("{module_qname}::{name}");
                    defs.push(Definition {
                        qualified_name: fqname.clone(),
                        kind: EntityKind::Function,
                        parent: None,
                    });
                    // Extract calls from function body
                    super::extract_calls_from_body(
                        &child,
                        source,
                        &fqname,
                        &mut calls,
                        &["block"],
                        extract_python_call,
                    );
                }
            }
            "class_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let class_name = node_text(&name_node, source);
                    defs.push(Definition {
                        qualified_name: format!("{module_qname}::{class_name}"),
                        kind: EntityKind::Type,
                        parent: None,
                    });
                    extract_python_class_methods(
                        &child,
                        source,
                        module_qname,
                        &class_name,
                        &mut defs,
                        &mut calls,
                    );
                }
            }
            "import_statement" => {
                let mut names = Vec::new();
                let mut cur = child.walk();
                for n in child.children(&mut cur) {
                    if n.kind() == "dotted_name" || n.kind() == "identifier" {
                        names.push(node_text(&n, source));
                    }
                }
                if !names.is_empty() {
                    imports.push(Import {
                        module_path: names.join(","),
                        imported_names: names,
                    });
                }
            }
            "import_from_statement" => {
                let mut module = String::new();
                let mut imported = Vec::new();
                let mut cur = child.walk();
                for n in child.children(&mut cur) {
                    match n.kind() {
                        "dotted_name" if module.is_empty() => {
                            module = node_text(&n, source);
                        }
                        "identifier" | "dotted_name" if !module.is_empty() => {
                            imported.push(node_text(&n, source));
                        }
                        _ => {}
                    }
                }
                if !module.is_empty() {
                    imports.push(Import {
                        module_path: module,
                        imported_names: imported,
                    });
                }
            }
            _ => {}
        }
    }

    (defs, imports, calls)
}

fn extract_python_class_methods(
    class_node: &Node,
    source: &str,
    module_qname: &str,
    class_name: &str,
    defs: &mut Vec<Definition>,
    calls: &mut Vec<Call>,
) {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "block" {
            let mut body_cursor = child.walk();
            for stmt in child.children(&mut body_cursor) {
                if stmt.kind() == "function_definition"
                    && let Some(name_node) = stmt.child_by_field_name("name")
                {
                    let method_name = node_text(&name_node, source);
                    let fqname = format!("{module_qname}::{class_name}::{method_name}");
                    defs.push(Definition {
                        qualified_name: fqname.clone(),
                        kind: EntityKind::Method,
                        parent: Some(class_name.to_owned()),
                    });
                    // Extract calls from method body
                    super::extract_calls_from_body(
                        &stmt,
                        source,
                        &fqname,
                        calls,
                        &["block"],
                        extract_python_call,
                    );
                }
            }
            break;
        }
    }
}

fn extract_python_call(node: &Node, source: &str) -> Option<String> {
    if node.kind() != "call" {
        return None;
    }
    let func = node.child(0)?;
    let callee = extract_callee_name(&func, source);
    if callee.is_empty() {
        None
    } else {
        Some(callee)
    }
}
/// Extract the simple function/method name from the expression node
/// that serves as the callable.  Handles:
///   `foo()`          → "foo"
///   `self.bar()`     → "bar"
///   `obj.method()`   → "method"
///   `Module.func()`  → "func"
fn extract_callee_name(func_node: &Node, source: &str) -> String {
    match func_node.kind() {
        "identifier" => node_text(func_node, source),
        "attribute" => {
            // Attribute: `self.method` — take the attribute name (last child)
            func_node
                .child_by_field_name("attribute")
                .map(|n| node_text(&n, source))
                .unwrap_or_default()
        }
        _ => String::new(),
    }
}
