use tree_sitter::{Node, TreeCursor};

use super::{Call, Definition, Import, node_text};
use crate::domain::EntityKind;

pub fn extract_js<'a>(
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
            "function_declaration" | "generator_function_declaration" => {
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
            "class_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let class_name = node_text(&name_node, source);
                    defs.push(Definition {
                        qualified_name: format!("{module_qname}::{class_name}"),
                        kind: EntityKind::Type,
                        parent: None,
                    });
                    extract_js_class_methods(
                        &child,
                        source,
                        module_qname,
                        &class_name,
                        &mut defs,
                        &mut calls,
                    );
                }
            }
            "export_statement" => {
                // Unwrap to find function/class declarations inside exports
                let mut cur = child.walk();
                for inner in child.children(&mut cur) {
                    if matches!(
                        inner.kind(),
                        "function_declaration"
                            | "generator_function_declaration"
                            | "class_declaration"
                            | "variable_declaration"
                    ) {
                        if inner.kind() == "variable_declaration" {
                            // Arrow functions assigned to const: only extract names
                            extract_js_var_decls(
                                &inner,
                                source,
                                module_qname,
                                &mut defs,
                                &mut calls,
                            );
                        } else if inner.kind() == "class_declaration" {
                            if let Some(name_node) = inner.child_by_field_name("name") {
                                let class_name = node_text(&name_node, source);
                                defs.push(Definition {
                                    qualified_name: format!("{module_qname}::{class_name}"),
                                    kind: EntityKind::Type,
                                    parent: None,
                                });
                                extract_js_class_methods(
                                    &inner,
                                    source,
                                    module_qname,
                                    &class_name,
                                    &mut defs,
                                    &mut calls,
                                );
                            }
                        } else if let Some(name_node) = inner.child_by_field_name("name") {
                            let name = node_text(&name_node, source);
                            let fqname = format!("{module_qname}::{name}");
                            defs.push(Definition {
                                qualified_name: fqname.clone(),
                                kind: EntityKind::Function,
                                parent: None,
                            });
                            extract_calls_from_block(&inner, source, &fqname, &mut calls);
                        }
                    }
                }
            }
            "import_statement" => {
                extract_js_import(&child, source, &mut imports);
            }
            _ => {}
        }
    }

    (defs, imports, calls)
}

fn extract_js_class_methods(
    class_node: &Node,
    source: &str,
    module_qname: &str,
    class_name: &str,
    defs: &mut Vec<Definition>,
    calls: &mut Vec<Call>,
) {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "class_body" || child.kind() == "body" {
            let mut body_cursor = child.walk();
            for member in child.children(&mut body_cursor) {
                if (member.kind() == "method_definition"
                    || member.kind() == "public_field_definition")
                    && let Some(name_node) = member.child_by_field_name("name")
                {
                    let method_name = node_text(&name_node, source);
                    let fqname = format!("{module_qname}::{class_name}::{method_name}");
                    defs.push(Definition {
                        qualified_name: fqname.clone(),
                        kind: EntityKind::Method,
                        parent: Some(class_name.to_owned()),
                    });
                    extract_calls_from_block(&member, source, &fqname, calls);
                }
            }
            break;
        }
    }
}

fn extract_js_var_decls(
    var_node: &Node,
    source: &str,
    module_qname: &str,
    defs: &mut Vec<Definition>,
    calls: &mut Vec<Call>,
) {
    let mut cursor = var_node.walk();
    for child in var_node.children(&mut cursor) {
        if child.kind() == "variable_declarator"
            && let Some(name_node) = child.child_by_field_name("name")
        {
            let name = node_text(&name_node, source);
            let fqname = format!("{module_qname}::{name}");
            // Check if it's an arrow function (has value that's an arrow_function)
            if let Some(val) = child.child_by_field_name("value")
                && (val.kind() == "arrow_function" || val.kind() == "function_expression")
            {
                defs.push(Definition {
                    qualified_name: fqname.clone(),
                    kind: EntityKind::Function,
                    parent: None,
                });
                extract_calls_from_block(&val, source, &fqname, calls);
            }
        }
    }
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
        if child.kind() == "statement_block" || child.kind() == "body" {
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

/// Extract the simple function/method name from a JS/TS call expression callee.
/// Handles `foo()`, `obj.method()`, `obj["method"]()`.
fn extract_callee_name(func_node: &Node, source: &str) -> String {
    match func_node.kind() {
        "identifier" => node_text(func_node, source),
        "member_expression" => {
            // `obj.method()` — take the property name
            func_node
                .child_by_field_name("property")
                .map(|n| node_text(&n, source))
                .unwrap_or_default()
        }
        "subscript_expression" => {
            // `obj["method"]()` — take the string literal inside
            let mut cur = func_node.walk();
            for child in func_node.children(&mut cur) {
                if child.kind() == "string" || child.kind() == "string_fragment" {
                    return node_text(&child, source);
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn extract_js_import(node: &Node, source: &str, imports: &mut Vec<Import>) {
    let mut module = String::new();
    let mut imported = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "string" | "string_fragment" => {
                module = node_text(&child, source)
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
            }
            "import_specifier" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    imported.push(node_text(&name_node, source));
                }
            }
            "namespace_import" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    imported.push(format!("*{}", node_text(&name_node, source)));
                }
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
