use tree_sitter::{Node, TreeCursor};

use super::{Definition, Import, node_text};
use crate::domain::EntityKind;

pub fn extract_python<'a>(
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
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    defs.push(Definition {
                        qualified_name: format!("{module_qname}::{name}"),
                        kind: EntityKind::Function,
                        parent: None,
                    });
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

    (defs, imports)
}

fn extract_python_class_methods(
    class_node: &Node,
    source: &str,
    module_qname: &str,
    class_name: &str,
    defs: &mut Vec<Definition>,
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
                    defs.push(Definition {
                        qualified_name: format!("{module_qname}::{class_name}::{method_name}"),
                        kind: EntityKind::Method,
                        parent: Some(class_name.to_owned()),
                    });
                }
            }
            break;
        }
    }
}
