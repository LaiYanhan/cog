use tree_sitter::{Node, TreeCursor};

use super::{Call, Definition, Import, node_text};
use crate::domain::EntityKind;

pub fn extract_java<'a>(
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
            "class_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let class_name = node_text(&name_node, source);
                    defs.push(Definition {
                        qualified_name: format!("{module_qname}::{class_name}"),
                        kind: EntityKind::Type,
                        parent: None,
                    });
                    extract_java_class_body(
                        &child,
                        source,
                        module_qname,
                        &class_name,
                        &mut defs,
                        &mut calls,
                    );
                }
            }
            "method_declaration" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = node_text(&name_node, source);
                    let fqname = format!("{module_qname}::{name}");
                    defs.push(Definition {
                        qualified_name: fqname.clone(),
                        kind: EntityKind::Function,
                        parent: None,
                    });
                    extract_calls_from_body(&child, source, &fqname, &mut calls);
                }
            }
            "import_declaration" => {
                let mut cur = child.walk();
                for n in child.children(&mut cur) {
                    if n.kind() == "scoped_identifier" || n.kind() == "identifier" {
                        let text = node_text(&n, source);
                        if !text.is_empty() {
                            imports.push(Import {
                                module_path: text,
                                imported_names: Vec::new(),
                            });
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    (defs, imports, calls)
}

fn extract_java_class_body(
    class_node: &Node,
    source: &str,
    module_qname: &str,
    class_name: &str,
    defs: &mut Vec<Definition>,
    calls: &mut Vec<Call>,
) {
    let mut cursor = class_node.walk();
    for child in class_node.children(&mut cursor) {
        if child.kind() == "class_body" || child.kind() == "interface_body" {
            let mut body_cursor = child.walk();
            for member in child.children(&mut body_cursor) {
                if member.kind() == "method_declaration"
                    && let Some(name_node) = member.child_by_field_name("name")
                {
                    let method_name = node_text(&name_node, source);
                    let fqname = format!("{module_qname}::{class_name}::{method_name}");
                    defs.push(Definition {
                        qualified_name: fqname.clone(),
                        kind: EntityKind::Method,
                        parent: Some(class_name.to_owned()),
                    });
                    extract_calls_from_body(&member, source, &fqname, calls);
                } else if member.kind() == "class_declaration"
                    && let Some(name_node) = member.child_by_field_name("name")
                {
                    let inner_name = node_text(&name_node, source);
                    defs.push(Definition {
                        qualified_name: format!("{module_qname}::{class_name}::{inner_name}"),
                        kind: EntityKind::Type,
                        parent: Some(class_name.to_owned()),
                    });
                    extract_java_class_body(
                        &member,
                        source,
                        module_qname,
                        &inner_name,
                        defs,
                        calls,
                    );
                }
            }
            break;
        }
    }
}

/// Walk a function body looking for `method_invocation` nodes.
fn extract_calls_from_body(
    func_node: &Node,
    source: &str,
    caller_qname: &str,
    calls: &mut Vec<Call>,
) {
    let mut cursor = func_node.walk();
    for child in func_node.children(&mut cursor) {
        if child.kind() == "block" || child.kind() == "body" {
            walk_for_calls(&child, source, caller_qname, calls);
            break;
        }
    }
}

/// Recursively walk looking for `method_invocation` nodes.
fn walk_for_calls(node: &Node, source: &str, caller_qname: &str, calls: &mut Vec<Call>) {
    if node.kind() == "method_invocation"
        && let Some(name_node) = node.child_by_field_name("name")
    {
        let callee = node_text(&name_node, source);
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
