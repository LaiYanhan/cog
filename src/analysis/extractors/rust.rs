use tree_sitter::{Node, TreeCursor};

use super::{Definition, Import, node_text};
use crate::domain::EntityKind;

pub fn extract_rust<'a>(
    root: &Node<'a>,
    source: &str,
    module_qname: &str,
    cursor: &mut TreeCursor<'a>,
) -> (Vec<Definition>, Vec<Import>) {
    let mut defs = Vec::new();
    let mut imports = Vec::new();

    for child in root.children(cursor) {
        process_rust_child(&child, source, module_qname, &mut defs, &mut imports);
    }

    (defs, imports)
}

fn process_rust_child<'a>(
    child: &Node<'a>,
    source: &str,
    module_qname: &str,
    defs: &mut Vec<Definition>,
    imports: &mut Vec<Import>,
) {
    match child.kind() {
        "function_item" => {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(&name_node, source);
                defs.push(Definition {
                    qualified_name: format!("{module_qname}::{name}"),
                    kind: EntityKind::Function,
                    parent: None,
                });
            }
        }
        "struct_item" | "enum_item" | "trait_item" => {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(&name_node, source);
                defs.push(Definition {
                    qualified_name: format!("{module_qname}::{name}"),
                    kind: EntityKind::Type,
                    parent: None,
                });
            }
        }
        "impl_item" => {
            extract_rust_impl(child, source, module_qname, defs);
        }
        "use_declaration" => {
            let mut cur = child.walk();
            for n in child.children(&mut cur) {
                if n.kind() != "use" && n.kind() != ";" && !n.kind().contains("use") {
                    let text = node_text(&n, source);
                    if !text.is_empty() {
                        imports.push(Import {
                            module_path: text,
                            imported_names: Vec::new(),
                        });
                    }
                }
            }
        }
        // Unwrap wrapper nodes: _declaration_statement, attribute_item, etc.
        _ if child.child_count() > 0 => {
            let mut inner_cur = child.walk();
            for inner in child.children(&mut inner_cur) {
                if matches!(
                    inner.kind(),
                    "function_item"
                        | "struct_item"
                        | "enum_item"
                        | "trait_item"
                        | "impl_item"
                        | "use_declaration"
                        | "attribute_item"
                        | "_declaration_statement"
                ) {
                    process_rust_child(&inner, source, module_qname, defs, imports);
                }
            }
        }
        _ => {}
    }
}

fn extract_rust_impl(
    impl_node: &Node,
    source: &str,
    module_qname: &str,
    defs: &mut Vec<Definition>,
) {
    let impl_name = impl_node
        .child_by_field_name("type")
        .map(|n| node_text(&n, source));

    // Methods are inside declaration_list, not direct children of impl_item
    let mut cursor = impl_node.walk();
    for child in impl_node.children(&mut cursor) {
        if child.kind() == "declaration_list" {
            let mut inner_cur = child.walk();
            for inner in child.children(&mut inner_cur) {
                if inner.kind() == "function_item"
                    && let Some(name_node) = inner.child_by_field_name("name")
                {
                    let fn_name = node_text(&name_node, source);
                    defs.push(Definition {
                        qualified_name: if let Some(iname) = &impl_name {
                            format!("{module_qname}::{iname}::{fn_name}")
                        } else {
                            format!("{module_qname}::{fn_name}")
                        },
                        kind: EntityKind::Method,
                        parent: impl_name.clone(),
                    });
                }
            }
        }
    }
}
