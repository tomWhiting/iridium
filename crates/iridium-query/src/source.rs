//! Validate JSON once and index source tokens by unambiguous member paths.
use crate::{MAX_NODES, SourceRange, Target};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Part {
    Key(String),
    Index(usize),
}

struct Located {
    value: SourceRange,
    key: Option<SourceRange>,
}

pub struct Index {
    entries: BTreeMap<Vec<Part>, Located>,
}

impl Index {
    pub(super) fn parse(source: &str, offset: usize, nodes: &mut usize) -> Result<Self, String> {
        // serde enforces JSON syntax and its bounded recursion limit. Tree-sitter
        // then supplies original source extents, preserving escapes and formatting.
        serde_json::from_str::<Value>(source).map_err(|error| format!("Invalid JSON: {error}"))?;
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_json::LANGUAGE.into())
            .map_err(|error| error.to_string())?;
        let tree = parser
            .parse(source, None)
            .ok_or("JSON source indexing was cancelled")?;
        if tree.root_node().has_error() {
            return Err("JSON source tree contains an error".into());
        }
        let root = tree
            .root_node()
            .named_child(0)
            .ok_or("JSON input is empty")?;
        let mut index = Self {
            entries: BTreeMap::new(),
        };
        index.walk(root, source, offset, &mut Vec::new(), None, nodes)?;
        Ok(index)
    }

    fn walk(
        &mut self,
        node: Node<'_>,
        source: &str,
        offset: usize,
        path: &mut Vec<Part>,
        key: Option<SourceRange>,
        nodes: &mut usize,
    ) -> Result<(), String> {
        *nodes += 1;
        if *nodes > MAX_NODES {
            return Err("JSON query exceeds 500,000 indexed values".into());
        }
        if path.len() > 128 {
            return Err("JSON nesting exceeds 128 levels".into());
        }
        self.entries.insert(
            path.clone(),
            Located {
                value: span(node, offset),
                key,
            },
        );
        let mut cursor = node.walk();
        match node.kind() {
            "object" => {
                let mut names = BTreeSet::new();
                for member in node.named_children(&mut cursor) {
                    let key = member
                        .child_by_field_name("key")
                        .ok_or("Object member has no key")?;
                    let value = member
                        .child_by_field_name("value")
                        .ok_or("Object member has no value")?;
                    let raw = source
                        .get(key.byte_range())
                        .ok_or("Invalid source key range")?;
                    let name: String =
                        serde_json::from_str(raw).map_err(|error| error.to_string())?;
                    if !names.insert(name.clone()) {
                        return Err(format!(
                            "Duplicate object key {name:?} makes query paths ambiguous"
                        ));
                    }
                    path.push(Part::Key(name));
                    self.walk(value, source, offset, path, Some(span(key, offset)), nodes)?;
                    path.pop();
                }
            },
            "array" => {
                for (index, child) in node.named_children(&mut cursor).enumerate() {
                    path.push(Part::Index(index));
                    self.walk(child, source, offset, path, None, nodes)?;
                    path.pop();
                }
            },
            _ => {},
        }
        Ok(())
    }

    pub(super) fn resolve(&self, path: &Value, target: Target) -> Result<SourceRange, String> {
        let parts = path.as_array().ok_or("jq did not return a source path")?;
        let path = parts
            .iter()
            .map(|part| match part {
                Value::String(key) => Ok(Part::Key(key.clone())),
                Value::Number(number) => {
                    let index = number
                        .as_u64()
                        .ok_or("jq array index must be a nonnegative integer")?;
                    usize::try_from(index)
                        .map(Part::Index)
                        .map_err(|error| format!("jq index out of range: {error}"))
                },
                _ => Err("jq source paths must contain object keys or array indices".to_owned()),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let found = self
            .entries
            .get(&path)
            .ok_or("jq addressed a value that does not exist in the source")?;
        match target {
            Target::Values => Ok(found.value),
            Target::Keys => found.key.ok_or_else(|| {
                "--keys requires matching object member values, not array elements or the root"
                    .into()
            }),
        }
    }
}

fn span(node: Node<'_>, offset: usize) -> SourceRange {
    SourceRange {
        start: offset + node.start_byte(),
        end: offset + node.end_byte(),
    }
}
