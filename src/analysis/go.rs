use tree_sitter::{Node, TreeCursor};

use super::{Definition, Import, node_text};
use crate::model::types::EntityKind;

pub fn extract_go<'a>(
    root: &Node<'a>,
    source: &str,
    module_qname: &str,
    cursor: &mut TreeCursor<'a>,
) -> (Vec<Definition>, Vec<Import>) {
    let mut defs = Vec::new();
    let mut imports = Vec::new();

    for child in root.children(cursor) {
        match child.kind() {
            "function_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    defs.push(Definition {
                        qualified_name: format!("{module_qname}::{name}"),
                        kind: EntityKind::Function,
                        parent: None,
                    });
                }
            }
            "method_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let method_name = node_text(&name_node, source);
                    let receiver = child.child_by_field_name("receiver").and_then(|r| {
                        let mut cur = r.walk();
                        r.children(&mut cur).find_map(|n| {
                            if n.kind() == "type_identifier" || n.kind() == "identifier" {
                                Some(node_text(&n, source))
                            } else {
                                None
                            }
                        })
                    });
                    defs.push(Definition {
                        qualified_name: match &receiver {
                            Some(recv) => format!("{module_qname}::{recv}::{method_name}"),
                            None => format!("{module_qname}::{method_name}"),
                        },
                        kind: EntityKind::Method,
                        parent: receiver,
                    });
                }
            }
            "type_declaration" => {
                let mut cur = child.walk();
                for n in child.children(&mut cur) {
                    if n.kind() == "type_spec"
                        && let Some(name_node) = n.child_by_field_name("name")
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

    (defs, imports)
}

fn extract_go_import(node: &Node, source: &str, imports: &mut Vec<Import>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "import_spec" => {
                if let Some(pn) = child.child_by_field_name("path") {
                    let text = node_text(&pn, source).trim_matches('"').to_owned();
                    if !text.is_empty() {
                        imports.push(Import {
                            module_path: text,
                            imported_names: Vec::new(),
                        });
                    }
                }
            }
            "import_spec_list" => {
                let mut sub = child.walk();
                for n in child.children(&mut sub) {
                    if n.kind() == "import_spec"
                        && let Some(pn) = n.child_by_field_name("path")
                    {
                        let text = node_text(&pn, source).trim_matches('"').to_owned();
                        if !text.is_empty() {
                            imports.push(Import {
                                module_path: text,
                                imported_names: Vec::new(),
                            });
                        }
                    }
                }
            }
            "interpreted_string_literal" => {
                let text = node_text(&child, source).trim_matches('"').to_owned();
                if !text.is_empty() {
                    imports.push(Import {
                        module_path: text,
                        imported_names: Vec::new(),
                    });
                }
            }
            _ => {}
        }
    }
}
