//! The message loop: documents in, answers out. Requests the server does not know are refused
//! with `MethodNotFound`, never ignored.

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use lsp_server::{Connection, ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CodeActionParams, CodeActionProviderCapability, CompletionOptions, CompletionParams,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentFormattingParams, GotoDefinitionParams, HoverParams, HoverProviderCapability, OneOf,
    PublishDiagnosticsParams, ReferenceParams, RenameOptions, RenameParams, SemanticTokenModifier,
    SemanticTokenType, SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensServerCapabilities, ServerCapabilities,
    TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, Uri,
};
use serde_json::{Value, json};
use syntaxeon_lexicon::lexicon::RhymeRules;

use crate::features::{self, Doc, GROUP_TOKENS};

type Failure = Box<dyn Error + Sync + Send>;

#[must_use]
pub fn capabilities() -> ServerCapabilities {
    // One token type per rhyme-group colour; the extension maps them to theme colours. A verse
    // that breaks its group also carries the `broken` modifier.
    let token_types = GROUP_TOKENS
        .iter()
        .map(|t| SemanticTokenType::new(t))
        .collect();
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions::default()),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: lsp_types::WorkDoneProgressOptions::default(),
        })),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types,
                    token_modifiers: vec![SemanticTokenModifier::new("broken")],
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: Some(false),
                ..SemanticTokensOptions::default()
            },
        )),
        ..ServerCapabilities::default()
    }
}

/// `file:///a%20b/c.sxe` → `/a b/c.sxe`; anything that is not a file URI has no path.
fn path_of(uri: &Uri) -> Option<PathBuf> {
    let rest = uri.as_str().strip_prefix("file://")?;
    let mut bytes = Vec::new();
    let raw = rest.as_bytes();
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'%'
            && let Some(hex) = rest.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            bytes.push(b);
            i += 3;
        } else {
            bytes.push(raw[i]);
            i += 1;
        }
    }
    let path = String::from_utf8(bytes).ok()?;
    // `/C:/dir/f.sxe` on Windows.
    let windows = path.len() > 2 && path.as_bytes()[0] == b'/' && path.as_bytes()[2] == b':';
    Some(PathBuf::from(if windows {
        path[1..].to_string()
    } else {
        path
    }))
}

struct Server<'a> {
    connection: &'a Connection,
    docs: HashMap<Uri, Doc>,
}

impl Server<'_> {
    fn send(&self, message: impl Into<Message>) -> Result<(), Failure> {
        self.connection.sender.send(message.into())?;
        Ok(())
    }

    fn open(&mut self, uri: Uri, text: String) -> Result<(), Failure> {
        let doc = Doc::new(path_of(&uri).as_deref(), text);
        let params = PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics: features::diagnostics(&doc),
            version: None,
        };
        self.docs.insert(uri, doc);
        self.send(Notification::new(
            "textDocument/publishDiagnostics".to_string(),
            params,
        ))
    }

    fn notification(&mut self, n: Notification) -> Result<(), Failure> {
        match n.method.as_str() {
            "textDocument/didOpen" => {
                let p: DidOpenTextDocumentParams = serde_json::from_value(n.params)?;
                self.open(p.text_document.uri, p.text_document.text)?;
            }
            "textDocument/didChange" => {
                let p: DidChangeTextDocumentParams = serde_json::from_value(n.params)?;
                // Full synchronisation: the last change is the whole text.
                if let Some(change) = p.content_changes.into_iter().last() {
                    self.open(p.text_document.uri, change.text)?;
                }
            }
            "textDocument/didClose" => {
                let p: DidCloseTextDocumentParams = serde_json::from_value(n.params)?;
                self.docs.remove(&p.text_document.uri);
                self.send(Notification::new(
                    "textDocument/publishDiagnostics".to_string(),
                    PublishDiagnosticsParams {
                        uri: p.text_document.uri,
                        diagnostics: Vec::new(),
                        version: None,
                    },
                ))?;
            }
            _ => {}
        }
        Ok(())
    }

    fn doc(&self, uri: &Uri) -> Result<&Doc, String> {
        self.docs
            .get(uri)
            .ok_or_else(|| format!("{} is not open", uri.as_str()))
    }

    /// The answer to a request, or the reason it cannot be answered.
    fn answer(&self, req: &Request) -> Result<Value, (ErrorCode, String)> {
        let bad = |e: serde_json::Error| (ErrorCode::InvalidParams, e.to_string());
        let params = req.params.clone();
        let open = |uri: &Uri| self.doc(uri).map_err(|m| (ErrorCode::InvalidParams, m));
        match req.method.as_str() {
            "textDocument/hover" => {
                let p: HoverParams = serde_json::from_value(params).map_err(bad)?;
                let t = p.text_document_position_params;
                let doc = open(&t.text_document.uri)?;
                Ok(json!(features::hover(doc, t.position)))
            }
            "textDocument/definition" => {
                let p: GotoDefinitionParams = serde_json::from_value(params).map_err(bad)?;
                let t = p.text_document_position_params;
                let doc = open(&t.text_document.uri)?;
                Ok(json!(features::definition(
                    doc,
                    &t.text_document.uri,
                    t.position
                )))
            }
            "textDocument/completion" => {
                let p: CompletionParams = serde_json::from_value(params).map_err(bad)?;
                let t = p.text_document_position;
                let doc = open(&t.text_document.uri)?;
                Ok(json!(features::completion(doc, t.position)))
            }
            "textDocument/references" => {
                let p: ReferenceParams = serde_json::from_value(params).map_err(bad)?;
                let t = p.text_document_position;
                let doc = open(&t.text_document.uri)?;
                Ok(json!(features::references(
                    doc,
                    &t.text_document.uri,
                    t.position,
                    p.context.include_declaration
                )))
            }
            "textDocument/prepareRename" => {
                let p: TextDocumentPositionParams = serde_json::from_value(params).map_err(bad)?;
                let doc = open(&p.text_document.uri)?;
                Ok(json!(features::prepare_rename(doc, p.position)))
            }
            "textDocument/rename" => {
                let p: RenameParams = serde_json::from_value(params).map_err(bad)?;
                let t = p.text_document_position;
                let doc = open(&t.text_document.uri)?;
                features::rename(doc, &t.text_document.uri, t.position, &p.new_name)
                    .map(|e| json!(e))
                    .map_err(|m| (ErrorCode::InvalidRequest, m))
            }
            "textDocument/formatting" => {
                let p: DocumentFormattingParams = serde_json::from_value(params).map_err(bad)?;
                let doc = open(&p.text_document.uri)?;
                Ok(json!(features::formatting(doc)))
            }
            "textDocument/codeAction" => {
                let p: CodeActionParams = serde_json::from_value(params).map_err(bad)?;
                let doc = open(&p.text_document.uri)?;
                Ok(json!(features::code_actions(
                    doc,
                    &p.text_document.uri,
                    p.range
                )))
            }
            "textDocument/semanticTokens/full" => {
                let p: SemanticTokensParams = serde_json::from_value(params).map_err(bad)?;
                let doc = open(&p.text_document.uri)?;
                Ok(json!(features::semantic_tokens(doc)))
            }
            // Custom requests for the audio layer of the extension (ADR-020).
            "syntaxeon/verses" => {
                let uri = params["textDocument"]["uri"]
                    .as_str()
                    .and_then(|s| s.parse::<Uri>().ok())
                    .ok_or((
                        ErrorCode::InvalidParams,
                        "textDocument.uri is required".to_string(),
                    ))?;
                Ok(features::verses_json(open(&uri)?))
            }
            "syntaxeon/compare" => {
                let word = |k: &str| {
                    params[k]
                        .as_str()
                        .map(str::to_lowercase)
                        .ok_or((ErrorCode::InvalidParams, format!("'{k}' is required")))
                };
                let (a, b) = (word("a")?, word("b")?);
                // Rules of an open document if one is named, else the defaults.
                let rules = params["textDocument"]["uri"]
                    .as_str()
                    .and_then(|s| s.parse::<Uri>().ok())
                    .and_then(|u| self.docs.get(&u))
                    .map_or(RhymeRules::default(), |d| d.rules);
                features::compare_json(rules, &a, &b).map_err(|m| (ErrorCode::InvalidParams, m))
            }
            other => Err((
                ErrorCode::MethodNotFound,
                format!("unknown request {other}"),
            )),
        }
    }

    fn request(&self, req: &Request) -> Result<(), Failure> {
        let response = match self.answer(req) {
            Ok(value) => Response::new_ok(req.id.clone(), value),
            Err((code, message)) => Response::new_err(req.id.clone(), code as i32, message),
        };
        self.send(response)
    }
}

/// Serves one client until it asks the server to shut down (or disconnects).
pub fn run(connection: &Connection) -> Result<(), Failure> {
    connection.initialize(serde_json::to_value(capabilities())?)?;
    let mut server = Server {
        connection,
        docs: HashMap::new(),
    };
    for message in &connection.receiver {
        match message {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                server.request(&req)?;
            }
            Message::Notification(n) => server.notification(n)?,
            Message::Response(_) => {}
        }
    }
    Ok(())
}
