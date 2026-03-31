//! LSP type definitions

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

/// A location inside a resource
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    /// The URI of the resource
    pub uri: String,
    /// The range within the resource
    pub range: Range,
}

/// Completion item kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionItemKind {
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

impl Default for CompletionItemKind {
    fn default() -> Self {
        Self::Text
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

impl Default for DiagnosticSeverity {
    fn default() -> Self {
        Self::Error
    }
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
