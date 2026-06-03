use tree_sitter::{Node, TreeCursor};

use super::{Definition, Import, node_text};
use crate::model::types::EntityKind;

pub fn extract_c<'a>(
    root: &Node<'a>,
    source: &str,
    module_qname: &str,
    cursor: &mut TreeCursor<'a>,
) -> (Vec<Definition>, Vec<Import>) {
    let mut defs = Vec::new();
    let mut imports = Vec::new();

    for child in root.children(cursor) {
        match child.kind() {
            "function_definition" => {
                if let Some(declarator) = child.child_by_field_name("declarator") {
                    let name = extract_c_declarator_name(&declarator, source);
                    if !name.is_empty() {
                        defs.push(Definition {
                            qualified_name: format!("{module_qname}::{name}"),
                            kind: EntityKind::Function,
                            parent: None,
                        });
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

    (defs, imports)
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
