//! The Piton language server.
//!
//! Every feature answers from the same compilation the `piton` binary would
//! produce, so the editor never disagrees with the compiler. The rules each
//! feature keeps are written in `spec/scope/lsp`, and this crate follows them.
//! The server is deliberately non-incremental: it re-analyses when something
//! asks after a change, and keeps one snapshot until the next change.

mod actions;
mod completion;
mod diff;
mod hover;
mod imports;
mod index;
mod line_index;
mod model;
mod navigation;
mod refactor;
mod tokens;
mod world;

#[cfg(test)]
mod model_tests;
#[cfg(test)]
mod protocol_tests;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use piton_core::diag::Severity;
use piton_core::value::AnchorId;
use piton_core::FileId;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::{Error, ErrorCode, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, ClientSocket, LanguageServer, LspService, Server};

use index::Located;
use world::{Registry, Snapshot, View, Workspace};

pub use world::Registry as FrameworkRegistry;

/// How long the editor has to stop changing documents before diagnostics are
/// published, so a burst of keystrokes compiles once.
const PUBLISH_DELAY: Duration = Duration::from_millis(150);

/// Run the language server over stdio until the client disconnects.
pub fn run(registry: Registry) -> anyhow::Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    runtime.block_on(async move {
        let (service, socket) = service(registry);
        Server::new(tokio::io::stdin(), tokio::io::stdout(), socket).serve(service).await;
    });
    Ok(())
}

/// The server, ready to be driven by an editor or by a test.
pub(crate) fn service(registry: Registry) -> (LspService<Backend>, ClientSocket) {
    LspService::build(move |client| Backend {
        client,
        workspace: Arc::new(Mutex::new(Workspace::new(registry))),
        published: Arc::new(Mutex::new(HashMap::new())),
        generation: Arc::new(AtomicU64::new(0)),
    })
    .finish()
}

pub(crate) struct Backend {
    client: Client,
    workspace: Arc<Mutex<Workspace>>,
    /// What the editor currently holds for each file.
    ///
    /// Only a change is sent, and a file whose last problem was fixed is sent
    /// an empty list, because an editor will not work out on its own that a
    /// squiggle should go away.
    published: Arc<Mutex<HashMap<Url, Vec<Diagnostic>>>>,
    /// Moves on with every change, so a publish scheduled before a later
    /// change gives way to the one scheduled after it.
    generation: Arc<AtomicU64>,
}

impl Backend {
    /// The project that analysed a document, and the document's id in it.
    ///
    /// A request names a file; which project answers for it is the workspace's
    /// to decide. A file no project analysed has no view, and the feature
    /// declines rather than guessing at one.
    async fn document(&self, url: &Url) -> Option<(Arc<View>, FileId)> {
        let path = url.to_file_path().ok()?;
        let snapshot = self.workspace.lock().await.snapshot();
        let (view, file) = snapshot.locate(&path)?;
        Some((view.clone(), file))
    }

    /// Resolve the file and offset a request points at.
    async fn locate(&self, url: &Url, position: Position) -> Option<(Arc<View>, FileId, piton_syntax::TextSize)> {
        let (view, file) = self.document(url).await?;
        let offset = view.line_index(file).offset(position);
        Some((view, file, offset))
    }

    fn location(view: &View, file: FileId, range: piton_syntax::TextRange) -> Option<Location> {
        let path = view.path_of(file)?;
        let url = Url::from_file_path(path).ok()?;
        Some(Location { uri: url, range: view.line_index(file).range(range) })
    }

    /// Publish diagnostics once the editor has been quiet for a moment.
    fn schedule_publish(&self) {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let latest = self.generation.clone();
        let client = self.client.clone();
        let workspace = self.workspace.clone();
        let published = self.published.clone();
        tokio::spawn(async move {
            tokio::time::sleep(PUBLISH_DELAY).await;
            if latest.load(Ordering::SeqCst) == generation {
                publish(&client, &workspace, &published).await;
            }
        });
    }
}

/// Send every file whose diagnostics changed since they were last sent.
async fn publish(
    client: &Client,
    workspace: &Mutex<Workspace>,
    published: &Mutex<HashMap<Url, Vec<Diagnostic>>>,
) {
    let snapshot = workspace.lock().await.snapshot();
    let current = all_diagnostics(&snapshot);
    let mut published = published.lock().await;
    for (url, diagnostics) in &current {
        if published.get(url) != Some(diagnostics) {
            client.publish_diagnostics(url.clone(), diagnostics.clone(), None).await;
        }
    }
    let cleared: Vec<Url> = published.keys().filter(|url| !current.contains_key(*url)).cloned().collect();
    for url in cleared {
        client.publish_diagnostics(url, Vec::new(), None).await;
    }
    *published = current;
}

/// Every file's diagnostics, each reported by the project that owns the file,
/// so nothing a sibling project thinks about a shared file reaches it twice.
fn all_diagnostics(snapshot: &Snapshot) -> HashMap<Url, Vec<Diagnostic>> {
    let mut out = HashMap::new();
    for view in snapshot.views() {
        for file in view.compilation.analysis.db.files() {
            let Some(path) = file.source.as_path() else { continue };
            let Some((owner, _)) = snapshot.locate(path) else { continue };
            if !Arc::ptr_eq(owner, view) {
                continue;
            }
            let diagnostics = file_diagnostics(view, file.id);
            if diagnostics.is_empty() {
                continue;
            }
            if let Ok(url) = Url::from_file_path(path) {
                out.insert(url, diagnostics);
            }
        }
    }
    out
}

/// The compiler's diagnostics for one file, and its unused imports as faded
/// hints.
fn file_diagnostics(view: &View, file: FileId) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = view
        .compilation
        .diagnostics
        .iter()
        .filter(|it| it.file == file)
        .map(|it| convert(view, it))
        .collect();
    let index = view.line_index(file);
    for unused in imports::unused(view, file) {
        out.push(Diagnostic {
            range: index.range(unused.range),
            severity: Some(DiagnosticSeverity::HINT),
            code: Some(NumberOrString::String("unused-import".to_string())),
            source: Some("piton".to_string()),
            message: unused.label,
            tags: Some(vec![DiagnosticTag::UNNECESSARY]),
            ..Diagnostic::default()
        });
    }
    out
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

/// A refusal the editor shows the author, such as why a rename cannot be made.
fn refusal(message: String) -> Error {
    Error { code: ErrorCode::InvalidRequest, message: message.into(), data: None }
}

/// The files this server wants to be told about: Piton sources and the
/// configurations that organise them.
fn piton_file_filters() -> FileOperationRegistrationOptions {
    let file = |glob: String| FileOperationFilter {
        scheme: Some("file".to_string()),
        pattern: FileOperationPattern {
            glob,
            matches: Some(FileOperationPatternKind::File),
            options: None,
        },
    };
    FileOperationRegistrationOptions {
        filters: vec![
            file("**/*.pi".to_string()),
            file(format!("**/{}", piton_core::project::CONFIG_FILE)),
            // A directory is moved as itself, not as the files under it, and
            // its name matches no source glob. Without a filter for folders the
            // client never mentions the move at all, and moving a directory of
            // modules would break every import that named them.
            FileOperationFilter {
                scheme: Some("file".to_string()),
                pattern: FileOperationPattern {
                    glob: "**".to_string(),
                    matches: Some(FileOperationPatternKind::Folder),
                    options: None,
                },
            },
        ],
    }
}

/// Read the moves out of a file-operation notification.
fn moves_of(params: &RenameFilesParams) -> Vec<refactor::Move> {
    params
        .files
        .iter()
        .filter_map(|file| {
            Some(refactor::Move {
                from: Url::parse(&file.old_uri).ok()?.to_file_path().ok()?,
                to: Url::parse(&file.new_uri).ok()?.to_file_path().ok()?,
            })
        })
        .collect()
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
                document_highlight_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
                    code_action_kinds: Some(vec![
                        CodeActionKind::QUICKFIX,
                        CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                        actions::REMOVE_UNUSED_IMPORTS,
                    ]),
                    resolve_provider: Some(false),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                })),
                inlay_hint_provider: Some(OneOf::Left(true)),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                // A move is a refactor: the editor asks what should change
                // before it moves anything, and the imports that named the file
                // are rewritten in the same undo step as the move itself.
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: None,
                    file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                        will_rename: Some(piton_file_filters()),
                        did_rename: Some(piton_file_filters()),
                        did_create: Some(piton_file_filters()),
                        did_delete: Some(piton_file_filters()),
                        ..WorkspaceFileOperationsServerCapabilities::default()
                    }),
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
        // What a project contains is read from the disk rather than from the
        // set of open buffers, so a file created, deleted, or moved by anything
        // other than an editor keystroke — a rename in the project panel, a
        // `git checkout`, another tool — changes the answer to every question
        // this server is asked. The client is asked to report every change to
        // a source or a configuration, wherever it lives.
        let watchers = serde_json::json!({
            "watchers": [
                { "globPattern": format!("**/{}", piton_core::project::CONFIG_FILE) },
                { "globPattern": "**/*.pi" },
            ]
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
                    register_options: Some(watchers),
                },
            ])
            .await;
        self.client.log_message(MessageType::INFO, "piton language server ready").await;
        publish(&self.client, &self.workspace, &self.published).await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        if let Ok(path) = params.text_document.uri.to_file_path() {
            self.workspace.lock().await.open(path, params.text_document.text);
        }
        self.schedule_publish();
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(change) = params.content_changes.into_iter().next_back() else { return };
        if let Ok(path) = params.text_document.uri.to_file_path() {
            self.workspace.lock().await.open(path, change.text);
        }
        self.schedule_publish();
    }

    async fn did_save(&self, _: DidSaveTextDocumentParams) {
        self.workspace.lock().await.invalidate();
        self.schedule_publish();
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        if let Ok(path) = params.text_document.uri.to_file_path() {
            self.workspace.lock().await.close(&path);
        }
        self.schedule_publish();
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let paths: Vec<PathBuf> =
            params.changes.iter().filter_map(|change| change.uri.to_file_path().ok()).collect();
        self.workspace.lock().await.changed_on_disk(&paths);
        self.schedule_publish();
    }

    // ---- files moving around ----------------------------------------------

    /// What should change before the editor moves these files.
    ///
    /// The edits go back as one workspace edit so the client applies them in
    /// the same undo step as the move: undoing a move that rewrote fifteen
    /// imports should not leave the fifteen behind.
    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        let moves = moves_of(&params);
        if moves.is_empty() {
            return Ok(None);
        }
        let edit = self.workspace.lock().await.will_move(&moves);
        Ok(match edit.changes.as_ref().is_some_and(|it| it.is_empty()) {
            true => None,
            false => Some(edit),
        })
    }

    async fn did_rename_files(&self, params: RenameFilesParams) {
        self.workspace.lock().await.moved(&moves_of(&params));
        self.schedule_publish();
    }

    async fn did_create_files(&self, _: CreateFilesParams) {
        self.workspace.lock().await.invalidate();
        self.schedule_publish();
    }

    async fn did_delete_files(&self, _: DeleteFilesParams) {
        self.workspace.lock().await.invalidate();
        self.schedule_publish();
    }

    // ---- diagnostics ------------------------------------------------------

    async fn diagnostic(
        &self,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let items = match self.document(&params.text_document.uri).await {
            Some((view, file)) => file_diagnostics(&view, file),
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
            for file in view.compilation.analysis.db.files() {
                let Some(path) = file.source.as_path() else { continue };
                let Some((owner, _)) = snapshot.locate(path) else { continue };
                if !Arc::ptr_eq(owner, view) {
                    continue;
                }
                let Ok(uri) = Url::from_file_path(path) else { continue };
                items.push(WorkspaceDocumentDiagnosticReport::Full(
                    WorkspaceFullDocumentDiagnosticReport {
                        uri,
                        version: None,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: None,
                            items: file_diagnostics(view, file.id),
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
        let position = params.text_document_position_params;
        let Some((view, file, offset)) = self.locate(&position.text_document.uri, position.position).await
        else {
            return Ok(None);
        };
        let Some(located) = navigation::locate(&view, file, offset) else { return Ok(None) };
        let mut locations: Vec<Location> = navigation::definitions(&view, file, &located)
            .into_iter()
            .filter_map(|(file, range)| Backend::location(&view, file, range))
            .collect();
        Ok(match locations.len() {
            0 => None,
            1 => locations.pop().map(GotoDefinitionResponse::Scalar),
            _ => Some(GotoDefinitionResponse::Array(locations)),
        })
    }

    async fn goto_implementation(
        &self,
        params: request::GotoImplementationParams,
    ) -> Result<Option<request::GotoImplementationResponse>> {
        let position = params.text_document_position_params;
        let Some((view, file, offset)) = self.locate(&position.text_document.uri, position.position).await
        else {
            return Ok(None);
        };
        let Some(located) = navigation::locate(&view, file, offset) else { return Ok(None) };
        let locations: Vec<Location> = navigation::implementations(&view, &located)
            .into_iter()
            .filter_map(|(file, range)| Backend::location(&view, file, range))
            .collect();
        Ok((!locations.is_empty()).then_some(GotoDefinitionResponse::Array(locations)))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let position = params.text_document_position;
        let Ok(path) = position.text_document.uri.to_file_path() else { return Ok(None) };
        let snapshot = self.workspace.lock().await.snapshot();
        let Some((view, file)) = snapshot.locate(&path) else { return Ok(None) };
        let offset = view.line_index(file).offset(position.position);
        let Some(Located::Symbol(occurrence)) = navigation::locate(view, file, offset) else {
            return Ok(Some(Vec::new()));
        };
        let mut locations: Vec<Location> = Vec::new();
        for other in snapshot.views() {
            for sym in navigation::counterparts(view, &occurrence.sym, other) {
                for (file, range) in navigation::references(other, &sym, params.context.include_declaration) {
                    if let Some(location) = Backend::location(other, file, range) {
                        if !locations.contains(&location) {
                            locations.push(location);
                        }
                    }
                }
            }
        }
        Ok(Some(locations))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let position = params.text_document_position_params;
        let Some((view, file, offset)) = self.locate(&position.text_document.uri, position.position).await
        else {
            return Ok(None);
        };
        let Some(located) = navigation::locate(&view, file, offset) else { return Ok(None) };
        let index = view.line_index(file);
        Ok(Some(
            navigation::highlights(&view, file, &located)
                .into_iter()
                .map(|(range, kind)| DocumentHighlight { range: index.range(range), kind: Some(kind) })
                .collect(),
        ))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let Some((view, file, offset)) = self.locate(&params.text_document.uri, params.position).await
        else {
            return Ok(None);
        };
        match refactor::prepare_rename(&view, file, offset) {
            Ok((range, placeholder)) => Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
                range: view.line_index(file).range(range),
                placeholder,
            })),
            Err(message) => Err(refusal(message)),
        }
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let position = params.text_document_position;
        let Ok(path) = position.text_document.uri.to_file_path() else { return Ok(None) };
        let snapshot = self.workspace.lock().await.snapshot();
        let Some((view, file)) = snapshot.locate(&path) else { return Ok(None) };
        let offset = view.line_index(file).offset(position.position);
        refactor::rename_edits(&snapshot, &path, offset, &params.new_name).map(Some).map_err(refusal)
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
        let mut out = Vec::new();
        // A file several projects analyse is listed once.
        let mut seen: HashSet<(PathBuf, piton_syntax::TextRange)> = HashSet::new();
        for view in snapshot.views() {
            for file in view.compilation.analysis.db.files() {
                let Some(path) = file.source.as_path() else { continue };
                let Ok(url) = Url::from_file_path(path) else { continue };
                let index = view.line_index(file.id);
                let container = view.display_path(path);
                let declared = file
                    .hir
                    .anchors
                    .iter()
                    .map(|it| {
                        let kind = if it.is_abstract { SymbolKind::INTERFACE } else { SymbolKind::CLASS };
                        (&it.name, it.name_range, kind)
                    })
                    .chain(file.hir.vars.iter().map(|it| (&it.name, it.name_range, SymbolKind::CONSTANT)));
                for (name, range, kind) in declared {
                    if !navigation::matches_query(&params.query, name) || !seen.insert((path.to_path_buf(), range)) {
                        continue;
                    }
                    out.push(workspace_symbol(
                        name.clone(),
                        kind,
                        Location { uri: url.clone(), range: index.range(range) },
                        container.clone(),
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
        let position = params.text_document_position_params;
        let Some((view, file, offset)) = self.locate(&position.text_document.uri, position.position).await
        else {
            return Ok(None);
        };
        let Some(id) = navigation::anchor_at(&view, file, offset) else { return Ok(None) };
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
        let position = params.text_document_position_params;
        let Some((view, file, offset)) = self.locate(&position.text_document.uri, position.position).await
        else {
            return Ok(None);
        };
        let Some(located) = navigation::locate(&view, file, offset) else { return Ok(None) };
        let Some(markdown) = hover::hover(&view, &located) else { return Ok(None) };
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent { kind: MarkupKind::Markdown, value: markdown }),
            range: Some(view.line_index(file).range(located.range())),
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let position = params.text_document_position;
        let Some((view, file, offset)) = self.locate(&position.text_document.uri, position.position).await
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
        let text = view.text(file);
        Ok(Some(diff::line_edits(text, &piton_fmt::format(text))))
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
        let only = params.context.only.as_deref();
        Ok(Some(actions::actions(&view, file, &params.text_document.uri, range, only)))
    }
}

#[allow(deprecated)]
fn workspace_symbol(name: String, kind: SymbolKind, location: Location, container: String) -> SymbolInformation {
    SymbolInformation {
        name,
        kind,
        tags: None,
        deprecated: None,
        location,
        container_name: Some(container),
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
