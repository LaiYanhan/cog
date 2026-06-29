use tree_sitter::{Node, TreeCursor};

use super::{Call, Definition, Import, node_text};
use crate::domain::EntityKind;

pub fn extract_rust<'a>(
    root: &Node<'a>,
    source: &str,
    module_qname: &str,
    cursor: &mut TreeCursor<'a>,
) -> (Vec<Definition>, Vec<Import>, Vec<Call>) {
    let mut defs = Vec::new();
    let mut imports = Vec::new();
    let mut calls = Vec::new();

    for child in root.children(cursor) {
        process_rust_child(
            &child,
            source,
            module_qname,
            &mut defs,
            &mut imports,
            &mut calls,
        );
    }

    (defs, imports, calls)
}

fn process_rust_child<'a>(
    child: &Node<'a>,
    source: &str,
    module_qname: &str,
    defs: &mut Vec<Definition>,
    imports: &mut Vec<Import>,
    calls: &mut Vec<Call>,
) {
    match child.kind() {
        "function_item" => {
            if let Some(name_node) = child.child_by_field_name("name") {
                let name = node_text(&name_node, source);
                let fqname = format!("{module_qname}::{name}");
                defs.push(Definition {
                    qualified_name: fqname.clone(),
                    kind: EntityKind::Function,
                    parent: None,
                });
                super::extract_calls_from_body(
                    child,
                    source,
                    &fqname,
                    calls,
                    &["block"],
                    extract_rust_call,
                );
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
            extract_rust_impl(child, source, module_qname, defs, calls);
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
                    process_rust_child(&inner, source, module_qname, defs, imports, calls);
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
    calls: &mut Vec<Call>,
) {
    let impl_name = impl_node
        .child_by_field_name("type")
        .map(|n| node_text(&n, source))
        // Strip generic parameters: `Lexer<'a>` -> `Lexer`, `HashMap<K,V>` -> `HashMap`.
        // The bare type name always precedes the first `<`, so splitting there is exact
        // and keeps method qnames stable across lifetime/generic changes.
        .map(|n| n.split('<').next().unwrap_or(&n).trim().to_string());

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
                    let fqname = if let Some(iname) = &impl_name {
                        format!("{module_qname}::{iname}::{fn_name}")
                    } else {
                        format!("{module_qname}::{fn_name}")
                    };
                    defs.push(Definition {
                        qualified_name: fqname.clone(),
                        kind: EntityKind::Method,
                        parent: impl_name.clone(),
                    });
                    super::extract_calls_from_body(
                        &inner,
                        source,
                        &fqname,
                        calls,
                        &["block"],
                        extract_rust_call,
                    );
                }
            }
        }
    }
}

fn extract_rust_call(node: &Node, source: &str) -> Option<String> {
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

/// Extract the simple function/method name from a Rust call expression callee.
/// Handles `function()`, `self.method()`, `Module::func()`, `obj.method()`.
fn extract_callee_name(func_node: &Node, source: &str) -> String {
    match func_node.kind() {
        "identifier" => node_text(func_node, source),
        "field_expression" => {
            // `self.method()` or `obj.method()` — take the field name
            func_node
                .child_by_field_name("field")
                .map(|n| node_text(&n, source))
                .unwrap_or_default()
        }
        "scoped_identifier" => {
            // `Module::func()` — take the name (last part)
            func_node
                .child_by_field_name("name")
                .map(|n| node_text(&n, source))
                .unwrap_or_default()
        }
        "generic_function" => {
            // `Vec::<T>::new()` — walk into the scoped_identifier inside
            let mut cur = func_node.walk();
            for child in func_node.children(&mut cur) {
                if child.kind() == "scoped_identifier"
                    || child.kind() == "field_expression"
                    || child.kind() == "identifier"
                {
                    return extract_callee_name(&child, source);
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}
