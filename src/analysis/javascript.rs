use tree_sitter::{Node, TreeCursor};

use super::{Definition, Import, node_text};
use crate::model::types::EntityKind;

pub fn extract_js<'a>(
    root: &Node<'a>,
    source: &str,
    module_qname: &str,
    cursor: &mut TreeCursor<'a>,
) -> (Vec<Definition>, Vec<Import>) {
    let mut defs = Vec::new();
    let mut imports = Vec::new();

    for child in root.children(cursor) {
        match child.kind() {
            "function_declaration" | "generator_function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    defs.push(Definition {
                        qualified_name: format!("{module_qname}::{name}"),
                        kind: EntityKind::Function,
                        parent: None,
                    });
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
                    extract_js_class_methods(&child, source, module_qname, &class_name, &mut defs);
                }
            }
            "export_statement" => {
                let mut cur = child.walk();
                for n in child.children(&mut cur) {
                    match n.kind() {
                        "function_declaration" | "generator_function_declaration" => {
                            if let Some(name_node) = n.child_by_field_name("name") {
                                let name = node_text(&name_node, source);
                                defs.push(Definition {
                                    qualified_name: format!("{module_qname}::{name}"),
                                    kind: EntityKind::Function,
                                    parent: None,
                                });
                            }
                        }
                        "class_declaration" => {
                            if let Some(name_node) = n.child_by_field_name("name") {
                                let class_name = node_text(&name_node, source);
                                defs.push(Definition {
                                    qualified_name: format!("{module_qname}::{class_name}"),
                                    kind: EntityKind::Type,
                                    parent: None,
                                });
                                extract_js_class_methods(
                                    &n,
                                    source,
                                    module_qname,
                                    &class_name,
                                    &mut defs,
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
            "import_statement" => {
                extract_js_import(&child, source, &mut imports);
            }
            _ => {}
        }
    }

    (defs, imports)
}

fn extract_js_class_methods(
    class_node: &Node,
    source: &str,
    module_qname: &str,
    class_name: &str,
    defs: &mut Vec<Definition>,
) {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "class_body" || child.kind() == "body" {
            let mut body_cursor = child.walk();
            for member in child.children(&mut body_cursor) {
                if member.kind() == "method_definition"
                    && let Some(name_node) = member.child_by_field_name("name")
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

fn extract_js_import(node: &Node, source: &str, imports: &mut Vec<Import>) {
    let mut module = String::new();
    let mut imported = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "string" => {
                if module.is_empty() {
                    let text = node_text(&child, source);
                    module = text.trim_matches(&['"', '\''][..]).to_owned();
                }
            }
            "identifier" => {
                imported.push(node_text(&child, source));
            }
            "named_imports" | "import_clause" => {
                let mut sub = child.walk();
                for n in child.children(&mut sub) {
                    if n.kind() == "identifier" {
                        imported.push(node_text(&n, source));
                    }
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
