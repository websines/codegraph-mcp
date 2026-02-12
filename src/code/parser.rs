use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};

use super::languages::LanguageConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
    Type,
    Const,
    Static,
    Variable,
    Module,
    Impl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceKind {
    Call,
    Import,
    Inherits,
    Implements,
    UsesType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line_start: u32,
    pub line_end: u32,
    pub signature: String,
    pub docstring: Option<String>,
    pub parent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedReference {
    pub from_symbol: Option<String>,
    pub to_name: String,
    pub kind: ReferenceKind,
    pub line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseResult {
    pub symbols: Vec<ExtractedSymbol>,
    pub references: Vec<ExtractedReference>,
}

/// Parse a file and extract symbols and references
pub fn parse_file(
    _path: &Path,
    source: &[u8],
    config: &LanguageConfig,
) -> Result<ParseResult> {
    // Create parser
    let mut parser = Parser::new();
    parser
        .set_language(&config.tree_sitter_language)
        .context("Failed to set parser language")?;

    // Parse source
    let tree = parser
        .parse(source, None)
        .context("Failed to parse source")?;

    let root_node = tree.root_node();

    // Extract symbols
    let symbols = extract_symbols(source, &root_node, config)?;

    // Extract references
    let mut references = extract_references(source, &root_node, config)?;

    // Resolve from_symbol: find enclosing function/method/class for each reference
    resolve_from_symbols(&symbols, &mut references);

    Ok(ParseResult {
        symbols,
        references,
    })
}

fn extract_symbols(
    source: &[u8],
    root_node: &tree_sitter::Node,
    config: &LanguageConfig,
) -> Result<Vec<ExtractedSymbol>> {
    let query = Query::new(&config.tree_sitter_language, config.queries.symbols)
        .context("Failed to create symbols query")?;

    let mut cursor = QueryCursor::new();
    let matches = cursor.matches(&query, *root_node, source);

    let mut symbols = Vec::new();

    for match_ in matches {
        let mut name = None;
        let mut kind = None;
        let mut node = None;

        for capture in match_.captures {
            let capture_name = &query.capture_names()[capture.index as usize];
            let text = capture.node.utf8_text(source).unwrap_or("");

            match capture_name.as_ref() {
                "name" => name = Some(text.to_string()),
                "function" => {
                    kind = Some(SymbolKind::Function);
                    node = Some(capture.node);
                }
                "method" => {
                    kind = Some(SymbolKind::Method);
                    node = Some(capture.node);
                }
                "class" => {
                    kind = Some(SymbolKind::Class);
                    node = Some(capture.node);
                }
                "struct" => {
                    kind = Some(SymbolKind::Struct);
                    node = Some(capture.node);
                }
                "enum" => {
                    kind = Some(SymbolKind::Enum);
                    node = Some(capture.node);
                }
                "interface" => {
                    kind = Some(SymbolKind::Interface);
                    node = Some(capture.node);
                }
                "trait" => {
                    kind = Some(SymbolKind::Trait);
                    node = Some(capture.node);
                }
                "type" => {
                    kind = Some(SymbolKind::Type);
                    node = Some(capture.node);
                }
                "const" => {
                    kind = Some(SymbolKind::Const);
                    node = Some(capture.node);
                }
                "static" => {
                    kind = Some(SymbolKind::Static);
                    node = Some(capture.node);
                }
                "variable" => {
                    kind = Some(SymbolKind::Variable);
                    node = Some(capture.node);
                }
                "module" => {
                    kind = Some(SymbolKind::Module);
                    node = Some(capture.node);
                }
                "impl" => {
                    kind = Some(SymbolKind::Impl);
                    node = Some(capture.node);
                }
                _ => {}
            }
        }

        if let (Some(name), Some(kind), Some(node)) = (name, kind, node) {
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            // Get signature (first line of the node)
            let signature = node
                .utf8_text(source)
                .unwrap_or("")
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();

            symbols.push(ExtractedSymbol {
                name,
                kind,
                line_start: start_pos.row as u32 + 1,
                line_end: end_pos.row as u32 + 1,
                signature,
                docstring: None,
                parent: None,
            });
        }
    }

    Ok(symbols)
}

fn extract_references(
    source: &[u8],
    root_node: &tree_sitter::Node,
    config: &LanguageConfig,
) -> Result<Vec<ExtractedReference>> {
    let query = Query::new(&config.tree_sitter_language, config.queries.references)
        .context("Failed to create references query")?;

    let mut cursor = QueryCursor::new();
    let matches = cursor.matches(&query, *root_node, source);

    let mut references = Vec::new();

    for match_ in matches {
        let mut name = None;
        let mut kind = None;
        let mut line = None;

        for capture in match_.captures {
            let capture_name = &query.capture_names()[capture.index as usize];
            let text = capture.node.utf8_text(source).unwrap_or("");

            match capture_name.as_ref() {
                "name" | "module" | "source" | "path" | "superclass" | "interface" | "trait" => {
                    name = Some(text.to_string());
                    line = Some(capture.node.start_position().row as u32 + 1);
                }
                "call" => kind = Some(ReferenceKind::Call),
                "import" | "use" => kind = Some(ReferenceKind::Import),
                "extends" => kind = Some(ReferenceKind::Inherits),
                "implements" => kind = Some(ReferenceKind::Implements),
                _ => {}
            }
        }

        if let (Some(name), Some(kind), Some(line)) = (name, kind, line) {
            references.push(ExtractedReference {
                from_symbol: None,
                to_name: name,
                kind,
                line,
            });
        }
    }

    Ok(references)
}

/// Resolve from_symbol for each reference by finding the tightest enclosing symbol
fn resolve_from_symbols(symbols: &[ExtractedSymbol], references: &mut [ExtractedReference]) {
    // Sort symbols by span size (smallest first) so we find the tightest enclosure
    let mut sorted_symbols: Vec<&ExtractedSymbol> = symbols
        .iter()
        .filter(|s| matches!(
            s.kind,
            SymbolKind::Function | SymbolKind::Method | SymbolKind::Class | SymbolKind::Struct | SymbolKind::Impl
        ))
        .collect();
    sorted_symbols.sort_by_key(|s| s.line_end - s.line_start);

    for reference in references.iter_mut() {
        // Find the smallest symbol that contains this reference line
        for symbol in &sorted_symbols {
            if reference.line >= symbol.line_start && reference.line <= symbol.line_end {
                reference.from_symbol = Some(symbol.name.clone());
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::languages::LANGUAGE_REGISTRY;

    #[test]
    fn test_parse_rust_function() {
        let source = b"fn hello_world() {\n    println!(\"Hello!\");\n}";
        let config = LANGUAGE_REGISTRY.get("rust").unwrap();

        let result = parse_file(Path::new("test.rs"), source, config).unwrap();

        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "hello_world");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn test_parse_typescript_class() {
        let source = b"class Foo {\n  bar() {}\n}";
        let config = LANGUAGE_REGISTRY.get("typescript").unwrap();

        let result = parse_file(Path::new("test.ts"), source, config).unwrap();

        assert!(result.symbols.len() >= 1);
        let class_sym = result.symbols.iter().find(|s| s.name == "Foo");
        assert!(class_sym.is_some());
        assert_eq!(class_sym.unwrap().kind, SymbolKind::Class);
    }

    #[test]
    fn test_parse_go_function_and_imports() {
        let source = b"package main\n\nimport \"fmt\"\n\nfunc Hello() {\n\tfmt.Println(\"hi\")\n}\n\ntype Server struct {\n\tPort int\n}\n\nfunc (s *Server) Start() {\n\tfmt.Println(\"starting\")\n}";
        let config = LANGUAGE_REGISTRY.get("go").unwrap();

        let result = parse_file(Path::new("test.go"), source, config).unwrap();

        // Symbols: Hello (function), Server (struct), Start (method)
        let func = result.symbols.iter().find(|s| s.name == "Hello");
        assert!(func.is_some(), "Should find Hello function");
        assert_eq!(func.unwrap().kind, SymbolKind::Function);

        let strct = result.symbols.iter().find(|s| s.name == "Server");
        assert!(strct.is_some(), "Should find Server struct");
        assert_eq!(strct.unwrap().kind, SymbolKind::Struct);

        let method = result.symbols.iter().find(|s| s.name == "Start");
        assert!(method.is_some(), "Should find Start method");
        assert_eq!(method.unwrap().kind, SymbolKind::Method);

        // References: import "fmt", calls to Println
        let imports: Vec<_> = result.references.iter().filter(|r| r.kind == ReferenceKind::Import).collect();
        assert!(!imports.is_empty(), "Should find Go imports");

        let calls: Vec<_> = result.references.iter().filter(|r| r.kind == ReferenceKind::Call).collect();
        assert!(!calls.is_empty(), "Should find Go method calls");
    }

    #[test]
    fn test_parse_python_function() {
        let source = b"def test_func():\n    pass";
        let config = LANGUAGE_REGISTRY.get("python").unwrap();

        let result = parse_file(Path::new("test.py"), source, config).unwrap();

        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "test_func");
        assert_eq!(result.symbols[0].kind, SymbolKind::Function);
    }
}
