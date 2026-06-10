use tree_sitter::{Node, TreeCursor};

use super::{Call, Definition, Import, node_text};
use crate::domain::EntityKind;

pub fn extract_c<'a>(
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
                if let Some(declarator) = child.child_by_field_name("declarator") {
                    let name = extract_c_declarator_name(&declarator, source);
                    if !name.is_empty() {
                        let fqname = format!("{module_qname}::{name}");
                        defs.push(Definition {
                            qualified_name: fqname.clone(),
                            kind: EntityKind::Function,
                            parent: None,
                        });
                        super::extract_calls_from_body(
                            &child,
                            source,
                            &fqname,
                            &mut calls,
                            &["compound_statement"],
                            extract_c_call,
                        );
                    }
                }
            }
            "struct_specifier" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    defs.push(Definition {
                        qualified_name: format!("{module_qname}::{name}"),
                        kind: EntityKind::Type,
                        parent: None,
                    });
                }
            }
            "preproc_include" => {
                let mut cur = child.walk();
                for n in child.children(&mut cur) {
                    if n.kind() == "string_literal" || n.kind() == "system_lib_string" {
                        let text = node_text(&n, source);
                        let trimmed = text.trim_matches(&['"', '<', '>', ' '][..]);
                        if !trimmed.is_empty() {
                            imports.push(Import {
                                module_path: trimmed.to_owned(),
                                imported_names: Vec::new(),
                            });
                        }
                    }
                }
            }
            "type_definition" => {
                if let Some(declarator) = child.child_by_field_name("declarator") {
                    let name = extract_c_declarator_name(&declarator, source);
                    if !name.is_empty() {
                        defs.push(Definition {
                            qualified_name: format!("{module_qname}::{name}"),
                            kind: EntityKind::Type,
                            parent: None,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    (defs, imports, calls)
}

fn extract_c_call(node: &Node, source: &str) -> Option<String> {
    if node.kind() != "call_expression" {
        return None;
    }
    let func = node.child_by_field_name("function")?;
    let callee = extract_callee_name(&func, source);
    if callee.is_empty() {
        None
    } else {
        Some(callee)
    }
}
/// Extract the simple function name from a C call expression callee.
fn extract_callee_name(func_node: &Node, source: &str) -> String {
    match func_node.kind() {
        "identifier" => node_text(func_node, source),
        "field_expression" => func_node
            .child_by_field_name("field")
            .map(|n| node_text(&n, source))
            .unwrap_or_default(),
        "pointer_expression" => {
            // *func_ptr() — walk to the inner identifier
            let mut cur = func_node.walk();
            for child in func_node.children(&mut cur) {
                if child.kind() == "identifier" {
                    return node_text(&child, source);
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

fn extract_c_declarator_name(node: &Node, source: &str) -> String {
    let mut current = *node;
    loop {
        match current.kind() {
            "identifier" => return node_text(&current, source),
            "pointer_declarator" | "function_declarator" => {
                if let Some(inner) = current.child_by_field_name("declarator") {
                    current = inner;
                } else {
                    let mut cur = current.walk();
                    let first = current.children(&mut cur).find(|n| {
                        n.kind() == "identifier"
                            || n.kind() == "pointer_declarator"
                            || n.kind() == "function_declarator"
                    });
                    match first {
                        Some(n) => current = n,
                        None => return String::new(),
                    }
                }
            }
            _ => return String::new(),
        }
    }
}
