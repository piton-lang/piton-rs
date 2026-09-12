//! The Piton language server.
//!
//! Every feature answers from the same compilation the `piton` binary would
//! produce, so the editor never disagrees with the compiler. The server is
//! deliberately non-incremental: it re-analyses on change and caches one
//! snapshot until the next edit.

mod actions;
mod completion;
mod hover;
mod line_index;
mod navigation;
mod tokens;
mod world;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use piton_core::diag::Severity;
use piton_core::resolve::Symbol;
use piton_core::value::AnchorId;
use piton_core::FileId;
use piton_syntax::kind::SyntaxKind;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use navigation::Target;
use world::{Registry, View, Workspace};

pub use world::Registry as FrameworkRegistry;

/// Run the language server over stdio until the client disconnects.
pub fn run(registry: Registry) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async move {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) =
            LspService::build(move |client| Backend {
                client,
                workspace: Mutex::new(Workspace::new(registry)),
                published: Mutex::new(std::collections::HashSet::new()),
            })
            .finish();
        Server::new(stdin, stdout, socket).serve(service).await;
    });
    Ok(())
}

struct Backend {
    client: Client,
    workspace: Mutex<Workspace>,
    /// The files the client currently holds diagnostics for.
    ///
    /// A file whose last problem was fixed needs an explicit empty publish, or
    /// the editor keeps showing a squiggle nothing will ever clear.
    published: Mutex<std::collections::HashSet<Url>>,
}

impl Backend {
    /// The project that analysed a document, and the document's id in it.
    ///
    /// A request names a file; which project answers for it is the workspace's
    /// to decide. A file no project analysed has no view, and the feature
    /// declines rather than guessing at one.
    async fn document(&self, url: &Url) -> Option<(std::sync::Arc<View>, FileId)> {
        let path = url.to_file_path().ok()?;
        let snapshot = self.workspace.lock().await.snapshot();
        let (view, file) = snapshot.locate(&path)?;
        Some((view.clone(), file))
    }

    /// Re-analyse and push diagnostics for every file the workspace touches.
    ///
    /// Each project reports on its own files and no others. A file is published
    /// once, from the compilation that owns it, so nothing a sibling project
    /// thinks about the same name can reach it.
    async fn publish(&self) {
        let (snapshot, open) = {
            let mut workspace = self.workspace.lock().await;
            (workspace.snapshot(), workspace.open_paths())
        };
        let mut current = std::collections::HashSet::new();
        for view in snapshot.views() {
            let mut per_file: std::collections::HashMap<FileId, Vec<Diagnostic>> =
                std::collections::HashMap::new();
            for diagnostic in view.compilation.diagnostics.iter() {
                per_file.entry(diagnostic.file).or_default().push(convert(view, diagnostic));
            }
            for file in view.compilation.analysis.db.files() {
                let Some(path) = file.source.as_path() else { continue };
                let Ok(url) = Url::from_file_path(path) else { continue };
                let diagnostics = per_file.remove(&file.id).unwrap_or_default();
                if diagnostics.is_empty() && !open.iter().any(|it| it == path) {
                    continue;
                }
                if !diagnostics.is_empty() {
                    current.insert(url.clone());
                }
                self.client.publish_diagnostics(url, diagnostics, None).await;
            }
        }

        // Anything that had problems last time and has none now has to be told
        // so explicitly; the editor will not work it out.
        let stale: Vec<Url> = {
            let mut published = self.published.lock().await;
            let stale = published.difference(&current).cloned().collect();
            *published = current;
            stale
        };
        for url in stale {
            self.client.publish_diagnostics(url, Vec::new(), None).await;
        }
    }

    /// Resolve the file and offset a request points at.
    async fn locate(
        &self,
        url: &Url,
        position: Position,
    ) -> Option<(std::sync::Arc<View>, FileId, piton_syntax::TextSize)> {
        let (view, file) = self.document(url).await?;
        let offset = view.line_index(file).offset(position);
        Some((view, file, offset))
    }

    fn location(view: &View, file: FileId, range: piton_syntax::TextRange) -> Option<Location> {
        let path = view.path_of(file)?;
        let url = Url::from_file_path(path).ok()?;
        Some(Location { uri: url, range: view.line_index(file).range(range) })
    }
}

fn convert(view: &View, diagnostic: &piton_core::diag::Diagnostic) -> Diagnostic {
    Diagnostic {
        range: view.line_index(diagnostic.file).range(diagnostic.range),
        severity: Some(match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
        }),
        code: Some(NumberOrString::String(diagnostic.code.to_string())),
        source: Some("piton".to_string()),
        message: diagnostic.message.clone(),
        ..Diagnostic::default()
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let mut roots: Vec<PathBuf> = params
            .workspace_folders
            .unwrap_or_default()
            .iter()
            .filter_map(|folder| folder.uri.to_file_path().ok())
            .collect();
        #[allow(deprecated)]
        if roots.is_empty() {
            if let Some(root) = params.root_uri.and_then(|uri| uri.to_file_path().ok()) {
                roots.push(root);
            }
        }
        self.workspace.lock().await.set_roots(roots);

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "piton".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        ":".to_string(),
                        "{".to_string(),
                        "$".to_string(),
                        "@".to_string(),
                        " ".to_string(),
                    ]),
                    ..CompletionOptions::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("piton".to_string()),
                        inter_file_dependencies: true,
                        workspace_diagnostics: true,
                        ..DiagnosticOptions::default()
                    },
                )),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: tokens::TOKEN_TYPES.to_vec(),
                                token_modifiers: tokens::TOKEN_MODIFIERS.to_vec(),
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..SemanticTokensOptions::default()
                        },
                    ),
                ),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        // `lsp-types` has no server capability for the type hierarchy, so it is
        // registered dynamically instead.
        let selector = serde_json::json!({
            "documentSelector": [{ "language": "piton" }, { "pattern": "**/*.pi" }]
        });
        // A configuration declares a project, so adding or editing one changes
        // what the workspace contains rather than just what one file says. The
        // client is asked to report every change to one, wherever it lives.
        let configs = serde_json::json!({
            "watchers": [{ "globPattern": format!("**/{}", piton_core::project::CONFIG_FILE) }]
        });
        let _ = self
            .client
            .register_capability(vec![
                Registration {
                    id: "piton-type-hierarchy".to_string(),
                    method: "textDocument/prepareTypeHierarchy".to_string(),
                    register_options: Some(selector),
                },
                Registration {
                    id: "piton-watch-config".to_string(),
                    method: "workspace/didChangeWatchedFiles".to_string(),
                    register_options: Some(configs),
                },
            ])
            .await;
        self.client.log_message(MessageType::INFO, "piton language server ready").await;
        self.publish().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        if let Ok(path) = params.text_document.uri.to_file_path() {
            self.workspace.lock().await.open(path, params.text_document.text);
        }
        self.publish().await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(change) = params.content_changes.into_iter().next_back() else { return };
        if let Ok(path) = params.text_document.uri.to_file_path() {
            self.workspace.lock().await.open(path, change.text);
        }
        self.publish().await;
    }

    async fn did_save(&self, _: DidSaveTextDocumentParams) {
        self.workspace.lock().await.invalidate();
        self.publish().await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Ok(path) = params.text_document.uri.to_file_path() {
            self.workspace.lock().await.close(&path);
        }
        self.publish().await;
    }

    async fn did_change_watched_files(&self, _: DidChangeWatchedFilesParams) {
        self.workspace.lock().await.invalidate();
        self.publish().await;
    }

    // ---- diagnostics ------------------------------------------------------

    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let items = match self.document(&params.text_document.uri).await {
            Some((view, file)) => view
                .compilation
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.file == file)
                .map(|diagnostic| convert(&view, diagnostic))
                .collect(),
            None => Vec::new(),
        };
        Ok(DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
            RelatedFullDocumentDiagnosticReport {
                related_documents: None,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: None,
                    items,
                },
            },
        )))
    }

    async fn workspace_diagnostic(
        &self,
        _: WorkspaceDiagnosticParams,
    ) -> Result<WorkspaceDiagnosticReportResult> {
        let snapshot = self.workspace.lock().await.snapshot();
        let mut items = Vec::new();
        for view in snapshot.views() {
            let mut per_file: std::collections::HashMap<FileId, Vec<Diagnostic>> =
                std::collections::HashMap::new();
            for diagnostic in view.compilation.diagnostics.iter() {
                per_file.entry(diagnostic.file).or_default().push(convert(view, diagnostic));
            }
            for file in view.compilation.analysis.db.files() {
                let Some(path) = file.source.as_path() else { continue };
                let Ok(uri) = Url::from_file_path(path) else { continue };
                items.push(WorkspaceDocumentDiagnosticReport::Full(
                    WorkspaceFullDocumentDiagnosticReport {
                        uri,
                        version: None,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: None,
                            items: per_file.remove(&file.id).unwrap_or_default(),
                        },
                    },
                ));
            }
        }
        Ok(WorkspaceDiagnosticReportResult::Report(WorkspaceDiagnosticReport { items }))
    }

    // ---- navigation -------------------------------------------------------

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let Some((view, file, offset)) = self
            .locate(
                &params.text_document_position_params.text_document.uri,
                params.text_document_position_params.position,
            )
            .await
        else {
            return Ok(None);
        };
        let Some(resolved) = navigation::resolve(&view, file, offset) else { return Ok(None) };
        let Some((target_file, range)) = navigation::definition(&view, &resolved.target) else {
            return Ok(None);
        };
        Ok(Backend::location(&view, target_file, range).map(GotoDefinitionResponse::Scalar))
    }

    async fn goto_implementation(
        &self,
        params: request::GotoImplementationParams,
    ) -> Result<Option<request::GotoImplementationResponse>> {
        let Some((view, file, offset)) = self
            .locate(
                &params.text_document_position_params.text_document.uri,
                params.text_document_position_params.position,
            )
            .await
        else {
            return Ok(None);
        };
        let Some(resolved) = navigation::resolve(&view, file, offset) else { return Ok(None) };
        let locations: Vec<Location> = navigation::implementations(&view, &resolved.target)
            .into_iter()
            .filter_map(|id| {
                let location = view.compilation.analysis.anchor_loc(id);
                Backend::location(
                    &view,
                    location.file,
                    view.compilation.analysis.anchor_def(id).name_range,
                )
            })
            .collect();
        Ok((!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations)))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let Some((view, file, offset)) = self
            .locate(&params.text_document_position.text_document.uri, params.text_document_position.position)
            .await
        else {
            return Ok(None);
        };
        let Some(resolved) = navigation::resolve(&view, file, offset) else { return Ok(None) };
        let mut locations: Vec<Location> = navigation::references(&view, &resolved.target)
            .into_iter()
            .filter_map(|(file, range)| Backend::location(&view, file, range))
            .collect();
        if params.context.include_declaration {
            if let Some((target_file, range)) = navigation::definition(&view, &resolved.target) {
                if let Some(location) = Backend::location(&view, target_file, range) {
                    if !locations.contains(&location) {
                        locations.push(location);
                    }
                }
            }
        }
        Ok(Some(locations))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let Some((view, file, offset)) =
            self.locate(&params.text_document.uri, params.position).await
        else {
            return Ok(None);
        };
        let Some(resolved) = navigation::resolve(&view, file, offset) else { return Ok(None) };
        if matches!(resolved.target, Target::Builtin(_) | Target::Module(_)) {
            return Ok(None);
        }
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: view.line_index(file).range(resolved.range),
            placeholder: resolved.text,
        }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let Some((view, file, offset)) = self
            .locate(&params.text_document_position.text_document.uri, params.text_document_position.position)
            .await
        else {
            return Ok(None);
        };
        let Some(resolved) = navigation::resolve(&view, file, offset) else { return Ok(None) };
        if !navigation::is_renameable(SyntaxKind::IDENT) {
            return Ok(None);
        }
        let mut sites = navigation::references(&view, &resolved.target);
        if let Some(declaration) = navigation::definition(&view, &resolved.target) {
            if !sites.contains(&declaration) {
                sites.push(declaration);
            }
        }
        let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
            std::collections::HashMap::new();
        for (site_file, range) in sites {
            let Some(path) = view.path_of(site_file) else { continue };
            let Ok(url) = Url::from_file_path(path) else { continue };
            changes.entry(url).or_default().push(TextEdit {
                range: view.line_index(site_file).range(range),
                new_text: params.new_name.clone(),
            });
        }
        Ok(Some(WorkspaceEdit { changes: Some(changes), ..WorkspaceEdit::default() }))
    }

    // ---- symbols ------------------------------------------------------------

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let Some((view, file)) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        Ok(Some(DocumentSymbolResponse::Nested(tokens::document_symbols(&view, file))))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let snapshot = self.workspace.lock().await.snapshot();
        let query = params.query.to_lowercase();
        let mut out = Vec::new();
        // A workspace symbol search is the one question that spans projects:
        // the user is asking where a name is, not what it resolves to.
        for view in snapshot.views() {
            for file in view.compilation.analysis.db.files() {
                let Some(path) = file.source.as_path() else { continue };
                let Ok(url) = Url::from_file_path(path) else { continue };
                let index = view.line_index(file.id);
                for anchor in &file.hir.anchors {
                    if !anchor.name.to_lowercase().contains(&query) {
                        continue;
                    }
                    out.push(workspace_symbol(
                        anchor.name.clone(),
                        if anchor.is_abstract { SymbolKind::INTERFACE } else { SymbolKind::CLASS },
                        Location { uri: url.clone(), range: index.range(anchor.name_range) },
                    ));
                }
                for variable in &file.hir.vars {
                    if !variable.name.to_lowercase().contains(&query) {
                        continue;
                    }
                    out.push(workspace_symbol(
                        variable.name.clone(),
                        SymbolKind::CONSTANT,
                        Location { uri: url.clone(), range: index.range(variable.name_range) },
                    ));
                }
            }
        }
        Ok(Some(out))
    }

    // ---- type hierarchy ------------------------------------------------------

    async fn prepare_type_hierarchy(
        &self,
        params: TypeHierarchyPrepareParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let Some((view, file, offset)) = self
            .locate(
                &params.text_document_position_params.text_document.uri,
                params.text_document_position_params.position,
            )
            .await
        else {
            return Ok(None);
        };
        let Some(resolved) = navigation::resolve(&view, file, offset) else { return Ok(None) };
        let id = match resolved.target {
            Target::Symbol(Symbol::Anchor(id)) | Target::Keyword(id) => id,
            _ => return Ok(None),
        };
        Ok(hierarchy_item(&view, id).map(|item| vec![item]))
    }

    async fn supertypes(
        &self,
        params: TypeHierarchySupertypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        // An anchor id means something only in the compilation that issued it,
        // so the item's own file says which project to ask.
        let Some((view, _)) = self.document(&params.item.uri).await else { return Ok(None) };
        let Some(id) = hierarchy_id(&view, &params.item) else { return Ok(None) };
        Ok(Some(
            view.compilation
                .analysis
                .bases(id)
                .into_iter()
                .filter_map(|base| hierarchy_item(&view, base))
                .collect(),
        ))
    }

    async fn subtypes(
        &self,
        params: TypeHierarchySubtypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let Some((view, _)) = self.document(&params.item.uri).await else { return Ok(None) };
        let Some(id) = hierarchy_id(&view, &params.item) else { return Ok(None) };
        Ok(Some(
            view.compilation
                .analysis
                .subtypes(id)
                .into_iter()
                .filter_map(|child| hierarchy_item(&view, child))
                .collect(),
        ))
    }

    // ---- text features --------------------------------------------------------

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let Some((view, file, offset)) = self
            .locate(
                &params.text_document_position_params.text_document.uri,
                params.text_document_position_params.position,
            )
            .await
        else {
            return Ok(None);
        };
        let Some(resolved) = navigation::resolve(&view, file, offset) else { return Ok(None) };
        let Some(markdown) = hover::hover(&view, &resolved) else { return Ok(None) };
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: Some(view.line_index(file).range(resolved.range)),
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let Some((view, file, offset)) = self
            .locate(&params.text_document_position.text_document.uri, params.text_document_position.position)
            .await
        else {
            return Ok(None);
        };
        Ok(Some(CompletionResponse::Array(completion::complete(&view, file, offset))))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let Some((view, file)) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens::semantic_tokens(&view, file),
        })))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let Some((view, file)) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        Ok(Some(tokens::folding_ranges(&view, file)))
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let Some((view, file)) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        Ok(Some(tokens::document_links(&view, file)))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let Some((view, file)) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        let index = view.line_index(file);
        let start = index.offset(params.range.start);
        let end = index.offset(params.range.end);
        Ok(Some(
            tokens::inlay_hints(&view, file)
                .into_iter()
                .filter(|hint| {
                    let at = index.offset(hint.position);
                    at >= start && at <= end
                })
                .collect(),
        ))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some((view, file)) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        let formatted = piton_fmt::format(view.text(file));
        if formatted == view.text(file) {
            return Ok(Some(Vec::new()));
        }
        Ok(Some(vec![TextEdit {
            range: view.line_index(file).full_range(),
            new_text: formatted,
        }]))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let Some((view, file)) = self.document(&params.text_document.uri).await else {
            return Ok(None);
        };
        let index = view.line_index(file);
        let range = piton_syntax::TextRange::new(
            index.offset(params.range.start),
            index.offset(params.range.end),
        );
        Ok(Some(actions::actions(&view, file, &params.text_document.uri, range)))
    }
}

#[allow(deprecated)]
fn workspace_symbol(name: String, kind: SymbolKind, location: Location) -> SymbolInformation {
    SymbolInformation {
        name,
        kind,
        tags: None,
        deprecated: None,
        location,
        container_name: None,
    }
}

/// A type-hierarchy item, with the anchor's id smuggled through `data`.
fn hierarchy_item(view: &View, id: AnchorId) -> Option<TypeHierarchyItem> {
    let analysis = &view.compilation.analysis;
    let location = analysis.anchor_loc(id);
    let definition = analysis.anchor_def(id);
    let path = view.path_of(location.file)?;
    let index = view.line_index(location.file);
    Some(TypeHierarchyItem {
        name: definition.name.clone(),
        kind: if definition.is_abstract { SymbolKind::INTERFACE } else { SymbolKind::CLASS },
        tags: None,
        detail: definition.keyword.as_ref().map(|it| format!("keyword `{}`", it.value)),
        uri: Url::from_file_path(path).ok()?,
        range: index.range(definition.range),
        selection_range: index.range(definition.name_range),
        data: Some(serde_json::Value::from(id.0)),
    })
}

fn hierarchy_id(view: &View, item: &TypeHierarchyItem) -> Option<AnchorId> {
    let raw = item.data.as_ref()?.as_u64()? as u32;
    view.compilation.analysis.anchor_ids().find(|id| id.0 == raw)
}
