//! LSP type definitions

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_kind_serialization() {
        // SymbolKind serializes to LSP integer codes
        assert_eq!(serde_json::to_value(SymbolKind::File).unwrap(), 1);
        assert_eq!(serde_json::to_value(SymbolKind::Module).unwrap(), 2);
        assert_eq!(serde_json::to_value(SymbolKind::Namespace).unwrap(), 3);
        assert_eq!(serde_json::to_value(SymbolKind::Package).unwrap(), 4);
        assert_eq!(serde_json::to_value(SymbolKind::Class).unwrap(), 5);
        assert_eq!(serde_json::to_value(SymbolKind::Method).unwrap(), 6);
        assert_eq!(serde_json::to_value(SymbolKind::Property).unwrap(), 7);
        assert_eq!(serde_json::to_value(SymbolKind::Field).unwrap(), 8);
        assert_eq!(serde_json::to_value(SymbolKind::Constructor).unwrap(), 9);
        assert_eq!(serde_json::to_value(SymbolKind::Enum).unwrap(), 10);
        assert_eq!(serde_json::to_value(SymbolKind::Interface).unwrap(), 11);
        assert_eq!(serde_json::to_value(SymbolKind::Function).unwrap(), 12);
        assert_eq!(serde_json::to_value(SymbolKind::Variable).unwrap(), 13);
        assert_eq!(serde_json::to_value(SymbolKind::Constant).unwrap(), 14);
        assert_eq!(serde_json::to_value(SymbolKind::String).unwrap(), 15);
        assert_eq!(serde_json::to_value(SymbolKind::Number).unwrap(), 16);
        assert_eq!(serde_json::to_value(SymbolKind::Boolean).unwrap(), 17);
        assert_eq!(serde_json::to_value(SymbolKind::Array).unwrap(), 18);
        assert_eq!(serde_json::to_value(SymbolKind::Object).unwrap(), 19);
        assert_eq!(serde_json::to_value(SymbolKind::Key).unwrap(), 20);
        assert_eq!(serde_json::to_value(SymbolKind::Null).unwrap(), 21);
        assert_eq!(serde_json::to_value(SymbolKind::EnumMember).unwrap(), 22);
        assert_eq!(serde_json::to_value(SymbolKind::Struct).unwrap(), 23);
        assert_eq!(serde_json::to_value(SymbolKind::Event).unwrap(), 24);
        assert_eq!(serde_json::to_value(SymbolKind::Operator).unwrap(), 25);
        assert_eq!(serde_json::to_value(SymbolKind::TypeParameter).unwrap(), 26);
    }

    #[test]
    fn test_symbol_kind_deserialization() {
        // SymbolKind deserializes from LSP integer codes
        assert_eq!(
            serde_json::from_value::<SymbolKind>(1u32.into()).unwrap(),
            SymbolKind::File
        );
        assert_eq!(
            serde_json::from_value::<SymbolKind>(12u32.into()).unwrap(),
            SymbolKind::Function
        );
        assert_eq!(
            serde_json::from_value::<SymbolKind>(26u32.into()).unwrap(),
            SymbolKind::TypeParameter
        );
    }

    #[test]
    fn test_document_symbol_with_children() {
        let child = DocumentSymbol {
            name: "child_method".to_string(),
            kind: SymbolKind::Method,
            detail: Some("fn()".to_string()),
            range: Range::new(Position::new(5, 0), Position::new(5, 20)),
            selection_range: Range::new(Position::new(5, 0), Position::new(5, 12)),
            children: None,
            tags: None,
            deprecated: None,
        };

        let parent = DocumentSymbol {
            name: "MyClass".to_string(),
            kind: SymbolKind::Class,
            detail: Some("struct MyClass".to_string()),
            range: Range::new(Position::new(0, 0), Position::new(10, 0)),
            selection_range: Range::new(Position::new(0, 0), Position::new(0, 7)),
            children: Some(vec![child]),
            tags: None,
            deprecated: None,
        };

        let json = serde_json::to_string(&parent).unwrap();
        let deserialized: DocumentSymbol = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.name, "MyClass");
        assert_eq!(deserialized.kind, SymbolKind::Class);
        assert!(deserialized.children.is_some());
        assert_eq!(deserialized.children.as_ref().unwrap().len(), 1);
        assert_eq!(
            deserialized.children.as_ref().unwrap()[0].name,
            "child_method"
        );
    }

    #[test]
    fn test_document_symbol_params_serialization() {
        let params = DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///src/main.rs".to_string(),
            },
            work_done_progress_params: None,
            partial_result_params: None,
        };

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"textDocument\""));
        assert!(json.contains("file:///src/main.rs"));
    }

    #[test]
    fn test_document_symbol_response_hierarchical() {
        // Test hierarchical DocumentSymbol[] response
        let json = r#"[
            {
                "name": "MyStruct",
                "kind": 23,
                "range": {"start": {"line": 0, "character": 0}, "end": {"line": 10, "character": 0}},
                "selectionRange": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 8}},
                "children": []
            }
        ]"#;

        let response: DocumentSymbolResponse = serde_json::from_str(json).unwrap();
        match response {
            DocumentSymbolResponse::Hierarchical(symbols) => {
                assert_eq!(symbols.len(), 1);
                assert_eq!(symbols[0].name, "MyStruct");
                assert_eq!(symbols[0].kind, SymbolKind::Struct);
            }
            DocumentSymbolResponse::Flat(_) => panic!("Expected hierarchical response"),
        }
    }

    #[test]
    fn test_document_symbol_response_flat() {
        // Test flat SymbolInformation[] response (no range field at top level)
        let json = r#"[
            {
                "name": "my_function",
                "kind": 12,
                "location": {"uri": "file:///src/main.rs", "range": {"start": {"line": 0, "character": 0}, "end": {"line": 5, "character": 0}}}
            }
        ]"#;

        let response: DocumentSymbolResponse = serde_json::from_str(json).unwrap();
        match response {
            DocumentSymbolResponse::Flat(symbols) => {
                assert_eq!(symbols.len(), 1);
                assert_eq!(symbols[0].name, "my_function");
            }
            DocumentSymbolResponse::Hierarchical(_) => panic!("Expected flat response"),
        }
    }

    #[test]
    fn test_did_open_params_serialization() {
        let params = TextDocumentDidOpenParams {
            text_document: TextDocumentItem {
                uri: "file:///src/main.rs".to_string(),
                language_id: "rust".to_string(),
                version: 1,
                text: "fn main() {}".to_string(),
            },
        };

        let json = serde_json::to_string(&params).unwrap();
        assert!(json.contains("\"textDocument\""));
        assert!(json.contains("\"languageId\":"));
        assert!(json.contains("\"version\":"));
    }

    #[test]
    fn test_server_capabilities_document_symbol_provider() {
        let caps = ServerCapabilities {
            document_symbol_provider: Some(true),
            ..Default::default()
        };

        let json = serde_json::to_string(&caps).unwrap();
        assert!(json.contains("\"documentSymbolProvider\""));
    }

    #[test]
    fn test_lsp_message_document_symbol() {
        let msg = LspMessage::TextDocumentDocumentSymbol(DocumentSymbolParams {
            text_document: TextDocumentIdentifier {
                uri: "file:///src/main.rs".to_string(),
            },
            work_done_progress_params: None,
            partial_result_params: None,
        });

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("textDocument/documentSymbol"));
    }
}

use serde::{Deserialize, Serialize};

/// Position in a text document
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// Line position in a document (0-based)
    pub line: u32,
    /// Character offset on a line in a document (0-based)
    pub character: u32,
}

impl Position {
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// Range in a text document
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    /// The start of the range
    pub start: Position,
    /// The end of the range
    pub end: Position,
}

impl Range {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }
}

/// Symbol kind as defined by the LSP specification
/// Serializes/deserializes as integer values per LSP spec
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    File = 1,
    Module = 2,
    Namespace = 3,
    Package = 4,
    Class = 5,
    Method = 6,
    Property = 7,
    Field = 8,
    Constructor = 9,
    Enum = 10,
    Interface = 11,
    Function = 12,
    Variable = 13,
    Constant = 14,
    String = 15,
    Number = 16,
    Boolean = 17,
    Array = 18,
    Object = 19,
    Key = 20,
    Null = 21,
    EnumMember = 22,
    Struct = 23,
    Event = 24,
    Operator = 25,
    TypeParameter = 26,
}

impl Serialize for SymbolKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_i32(*self as i32)
    }
}

impl<'de> Deserialize<'de> for SymbolKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = i32::deserialize(deserializer)?;
        match value {
            1 => Ok(SymbolKind::File),
            2 => Ok(SymbolKind::Module),
            3 => Ok(SymbolKind::Namespace),
            4 => Ok(SymbolKind::Package),
            5 => Ok(SymbolKind::Class),
            6 => Ok(SymbolKind::Method),
            7 => Ok(SymbolKind::Property),
            8 => Ok(SymbolKind::Field),
            9 => Ok(SymbolKind::Constructor),
            10 => Ok(SymbolKind::Enum),
            11 => Ok(SymbolKind::Interface),
            12 => Ok(SymbolKind::Function),
            13 => Ok(SymbolKind::Variable),
            14 => Ok(SymbolKind::Constant),
            15 => Ok(SymbolKind::String),
            16 => Ok(SymbolKind::Number),
            17 => Ok(SymbolKind::Boolean),
            18 => Ok(SymbolKind::Array),
            19 => Ok(SymbolKind::Object),
            20 => Ok(SymbolKind::Key),
            21 => Ok(SymbolKind::Null),
            22 => Ok(SymbolKind::EnumMember),
            23 => Ok(SymbolKind::Struct),
            24 => Ok(SymbolKind::Event),
            25 => Ok(SymbolKind::Operator),
            26 => Ok(SymbolKind::TypeParameter),
            _ => Err(serde::de::Error::custom(format!(
                "Unknown SymbolKind value: {}",
                value
            ))),
        }
    }
}

/// A document symbol with hierarchical children
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbol {
    /// The name of this symbol
    pub name: String,
    /// The kind of this symbol
    pub kind: SymbolKind,
    /// More detail for this symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The range enclosing this symbol
    pub range: Range,
    /// The range that should be selected
    pub selection_range: Range,
    /// Children of this symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<DocumentSymbol>>,
    /// Tags for this symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<SymbolTag>>,
    /// Whether this symbol is deprecated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
}

/// Symbol tag for deprecated symbols
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolTag {
    Deprecated = 1,
}

/// Flat symbol information as returned by some LSP servers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInformation {
    /// The name of this symbol
    pub name: String,
    /// The kind of this symbol
    pub kind: SymbolKind,
    /// The location of this symbol
    pub location: Location,
    /// The name of the containing symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    /// Tags for this symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<SymbolTag>>,
    /// Whether this symbol is deprecated
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated: Option<bool>,
}

/// Document symbol response can be either hierarchical or flat
#[derive(Debug, Clone, Serialize)]
pub enum DocumentSymbolResponse {
    /// Hierarchical document symbols
    Hierarchical(Vec<DocumentSymbol>),
    /// Flat symbol information
    Flat(Vec<SymbolInformation>),
}

impl<'de> Deserialize<'de> for DocumentSymbolResponse {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // First parse as a JSON array to inspect the first element
        let json_value: serde_json::Value = serde::Deserialize::deserialize(deserializer)?;

        if let Some(arr) = json_value.as_array()
            && let Some(first) = arr.first()
        {
            // Hierarchical DocumentSymbol has "range" field
            // Flat SymbolInformation has "location" field
            if first.get("range").is_some() {
                let symbols: Vec<DocumentSymbol> = serde_json::from_value(json_value)
                    .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                return Ok(DocumentSymbolResponse::Hierarchical(symbols));
            } else if first.get("location").is_some() {
                let symbols: Vec<SymbolInformation> = serde_json::from_value(json_value)
                    .map_err(|e| serde::de::Error::custom(e.to_string()))?;
                return Ok(DocumentSymbolResponse::Flat(symbols));
            }
        }

        // Fallback: try hierarchical
        let symbols: Vec<DocumentSymbol> = serde_json::from_value(json_value.clone())
            .map_err(|e| serde::de::Error::custom(e.to_string()))?;
        Ok(DocumentSymbolResponse::Hierarchical(symbols))
    }
}

/// Parameters for document symbol request
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbolParams {
    /// The text document
    pub text_document: TextDocumentIdentifier,
    /// Work done progress (unused but part of LSP spec)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_done_progress_params: Option<WorkDoneProgressParams>,
    /// Partial result progress (unused but part of LSP spec)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_result_params: Option<PartialResultParams>,
}

/// Work done progress parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkDoneProgressParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_done_token: Option<ProgressToken>,
}

/// Partial result parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartialResultParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_result_token: Option<ProgressToken>,
}

/// Progress token for work done / partial result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProgressToken {
    Integer(i32),
    String(String),
}

/// Parameters for textDocument/didOpen notification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentDidOpenParams {
    /// The text document that was opened
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentItem,
}

/// A text document item sent during didOpen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentItem {
    /// The text document's URI
    pub uri: String,
    /// The text document's language identifier
    #[serde(rename = "languageId")]
    pub language_id: String,
    /// The version number of the document
    pub version: i32,
    /// The content of the document
    pub text: String,
}

/// A location inside a resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    /// The URI of the resource
    pub uri: String,
    /// The range within the resource
    pub range: Range,
}

/// Completion item kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CompletionItemKind {
    #[default]
    Text,
    Method,
    Function,
    Constructor,
    Field,
    Variable,
    Class,
    Interface,
    Module,
    Property,
    Unit,
    Value,
    Enum,
    Keyword,
    Snippet,
    Color,
    File,
    Reference,
    Folder,
    EnumMember,
    Constant,
    Struct,
    Event,
    Operator,
    TypeParameter,
}

/// A completion item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    /// The label of this completion item
    pub label: String,
    /// The kind of this completion item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<CompletionItemKind>,
    /// A human-readable string with additional information about this item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The documentation of this completion item
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<String>,
}

impl CompletionItem {
    pub fn new(label: String) -> Self {
        Self {
            label,
            kind: None,
            detail: None,
            documentation: None,
        }
    }

    pub fn with_kind(mut self, kind: CompletionItemKind) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn with_detail(mut self, detail: String) -> Self {
        self.detail = Some(detail);
        self
    }

    pub fn with_documentation(mut self, documentation: String) -> Self {
        self.documentation = Some(documentation);
        self
    }
}

/// The kind of a marking content
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkedString {
    /// Plain text
    String(String),
    /// Markdown string
    Markdown(String),
}

impl From<String> for MarkedString {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for MarkedString {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

/// Hover information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    /// The contents of the hover
    pub contents: MarkedString,
    /// An optional range inside the text document that is used to
    /// visualize the hover, e.g. by changing the background color.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}

/// Diagnostic severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    #[default]
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// Represents a diagnostic, such as a compiler error or warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// The range at which the message applies
    pub range: Range,
    /// The diagnostic's severity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<DiagnosticSeverity>,
    /// The diagnostic's message
    pub message: String,
    /// The source of the diagnostic
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Diagnostic {
    pub fn new(range: Range, message: String) -> Self {
        Self {
            range,
            severity: None,
            message,
            source: None,
        }
    }

    pub fn with_severity(mut self, severity: DiagnosticSeverity) -> Self {
        self.severity = Some(severity);
        self
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = Some(source);
        self
    }
}

/// Server capabilities
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_document_sync: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references_provider: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_symbol_provider: Option<bool>,
}

/// LSP Message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum LspMessage {
    // Lifecycle messages
    #[serde(rename = "initialize")]
    Initialize { capabilities: ClientCapabilities },

    #[serde(rename = "initialized")]
    Initialized,

    #[serde(rename = "shutdown")]
    Shutdown,

    #[serde(rename = "exit")]
    Exit,

    // Text document messages
    #[serde(rename = "textDocument/completion")]
    TextDocumentCompletion(TextDocumentPositionParams),

    #[serde(rename = "textDocument/hover")]
    TextDocumentHover(TextDocumentPositionParams),

    #[serde(rename = "textDocument/definition")]
    TextDocumentDefinition(TextDocumentPositionParams),

    #[serde(rename = "textDocument/references")]
    TextDocumentReferences(TextDocumentReferencesParams),

    #[serde(rename = "textDocument/publishDiagnostics")]
    PublishDiagnostics(PublishDiagnosticsParams),

    #[serde(rename = "textDocument/documentSymbol")]
    TextDocumentDocumentSymbol(DocumentSymbolParams),

    #[serde(rename = "textDocument/didOpen")]
    TextDocumentDidOpen(TextDocumentDidOpenParams),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_document: Option<TextDocumentClientCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentClientCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synchronization: Option<TextDocumentSyncCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion: Option<CompletionCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover: Option<HoverCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<DefinitionCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub references: Option<ReferenceCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentSyncCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub will_save: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did_save: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_item: Option<CompletionItemCapabilities>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItemCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet_support: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation_format: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HoverCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_registration: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinitionCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_registration: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceCapabilities {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dynamic_registration: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentPositionParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentReferencesParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
    pub context: ReferenceContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceContext {
    pub include_declaration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
}
