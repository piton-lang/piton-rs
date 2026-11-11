//! The Piton language server.
//!
//! Each feature here answers a question about the *resolved* program, not
//! about raw syntax. A hover on a user keyword finds the anchor that it
//! aliases. Go-to-definition on an inherited property goes to the base that
//! declares it. A rename of a symbol also changes the imports that carry it.

mod plugins;
pub mod analysis;
pub mod convert;
pub mod features;
pub mod index;
pub mod world;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use piton_syntax::language;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use world::World;

/// Semantic token types, in the order that the server gives them to the client.
///
/// The server colours the data that only the resolved program knows: the kind
/// of a name. The syntax definition colours comments, keywords, literals,
/// operators and prose. Thus they have no type here.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::CLASS,     // anchors
    SemanticTokenType::PROPERTY,  // properties and dictionary keys
    SemanticTokenType::VARIABLE,  // file-level variables
    SemanticTokenType::KEYWORD,   // user keywords
    SemanticTokenType::NAMESPACE, // module paths
];

/// Semantic token modifiers. The last three are specific to Piton. The first marks a
/// property with a value from a base. The second marks a declaration that
/// other files can import. The third marks a name that another file declares.
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::ABSTRACT,
    SemanticTokenModifier::DEFAULT_LIBRARY,
    SemanticTokenModifier::new("inherited"),
    SemanticTokenModifier::new("exported"),
    SemanticTokenModifier::new("imported"),
];

/// The reason for a recompile. It decides if the server must tell the
/// other open files to ask again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The user types in a buffer. The client asks again for that buffer itself.
    Edit,
    /// A save, a file change on disk, a closed buffer, a new configuration.
    /// The tokens, hints and lenses of other files can depend on these.
    Elsewhere,
}

/// Tells if a recompile for `trigger` must ask the client for a refresh of
/// all open files. The refresh covers semantic tokens, inlay hints and code lenses.
///
/// This does not occur after an edit. The client itself asks again for the
/// buffer that the user edits. A refresh of all other files on each keystroke
/// makes them flicker. Effects on other files show when the user saves.
pub fn refreshes_after(trigger: Trigger) -> bool {
    trigger == Trigger::Elsewhere
}

/// The folder of the server. This is the first workspace folder. If there is
/// none, it is the deprecated root URI that old clients still send. If there is
/// none, it is the directory where the server started.
pub fn workspace_root(params: &InitializeParams) -> Option<PathBuf> {
    params
        .workspace_folders
        .as_ref()
        .and_then(|folders| folders.first())
        .and_then(|folder| convert::url_to_path(&folder.uri))
        .or_else(|| {
            #[allow(deprecated)]
            params.root_uri.as_ref().and_then(convert::url_to_path)
        })
}

/// The features of the client, from its capabilities.
pub fn client_caps(capabilities: &ClientCapabilities) -> world::ClientCaps {
    let text = capabilities.text_document.as_ref();
    let workspace = capabilities.workspace.as_ref();
    world::ClientCaps {
        link_support: text
            .and_then(|text| text.definition.as_ref())
            .and_then(|definition| definition.link_support)
            .unwrap_or(false),
        document_changes: workspace
            .and_then(|workspace| workspace.workspace_edit.as_ref())
            .and_then(|edit| edit.document_changes)
            .unwrap_or(false),
        insert_replace: text
            .and_then(|text| text.completion.as_ref())
            .and_then(|completion| completion.completion_item.as_ref())
            .and_then(|item| item.insert_replace_support)
            .unwrap_or(false),
        semantic_refresh: workspace
            .and_then(|workspace| workspace.semantic_tokens.as_ref())
            .and_then(|tokens| tokens.refresh_support)
            .unwrap_or(false),
        inlay_refresh: workspace
            .and_then(|workspace| workspace.inlay_hint.as_ref())
            .and_then(|hints| hints.refresh_support)
            .unwrap_or(false),
        lens_refresh: workspace
            .and_then(|workspace| workspace.code_lens.as_ref())
            .and_then(|lenses| lenses.refresh_support)
            .unwrap_or(false),
    }
}

/// All features that the server offers. They are in one location, so that a
/// test can compare them to the design.
pub fn server_capabilities() -> ServerCapabilities {
    let pi_files = FileOperationRegistrationOptions {
        filters: vec![
            FileOperationFilter {
                scheme: Some("file".into()),
                pattern: FileOperationPattern {
                    glob: format!("**/*.{}", language::EXTENSION),
                    matches: Some(FileOperationPatternKind::File),
                    options: None,
                },
            },
            FileOperationFilter {
                scheme: Some("file".into()),
                pattern: FileOperationPattern {
                    glob: "**".into(),
                    matches: Some(FileOperationPatternKind::Folder),
                    options: None,
                },
            },
        ],
    };
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(TextDocumentSyncSaveOptions::Supported(true)),
                ..Default::default()
            },
        )),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![
                "{".into(),
                "$".into(),
                "@".into(),
                "#".into(),
                ".".into(),
                ":".into(),
                "/".into(),
            ]),
            // The server makes the list on each keystroke. The description of
            // an anchor renders its full compiled value. Thus the server makes
            // the description later, only for the item that the cursor selects.
            resolve_provider: Some(true),
            ..Default::default()
        }),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        definition_provider: Some(OneOf::Left(true)),
        type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
        implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
        references_provider: Some(OneOf::Left(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: Default::default(),
        })),
        document_symbol_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        // The user can press enter after a line that ends in a colon. Then
        // the cursor must go inside the block that the line opened. The grammar
        // does not include indentation. Thus the server does this.
        document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
            first_trigger_character: "\n".into(),
            more_trigger_character: None,
        }),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![
                CodeActionKind::QUICKFIX,
                CodeActionKind::REFACTOR,
                CodeActionKind::SOURCE,
                CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
            ]),
            work_done_progress_options: Default::default(),
            resolve_provider: Some(false),
        })),
        code_lens_provider: Some(CodeLensOptions {
            resolve_provider: Some(false),
        }),
        inlay_hint_provider: Some(OneOf::Left(true)),
        execute_command_provider: Some(ExecuteCommandOptions {
            commands: vec![
                features::SOURCE_TO_OUTPUT.into(),
                features::SHOW_LOCATION.into(),
            ],
            work_done_progress_options: Default::default(),
        }),
        semantic_tokens_provider: Some(SemanticTokensServerCapabilities::SemanticTokensOptions(
            SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: TOKEN_TYPES.to_vec(),
                    token_modifiers: TOKEN_MODIFIERS.to_vec(),
                },
                full: Some(SemanticTokensFullOptions::Bool(true)),
                range: None,
                ..Default::default()
            },
        )),
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: None,
            file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                will_rename: Some(pi_files),
                ..Default::default()
            }),
        }),
        ..Default::default()
    }
}

/// The duration of a pause in keystrokes before the server recompiles by itself.
///
/// A request (hover, completion, ...) never waits for this. It compiles first
/// if the buffers changed after the last compilation. The pause only decides
/// when the server publishes diagnostics again while a person types.
const DEBOUNCE: Duration = Duration::from_millis(250);

pub struct Backend {
    client: Client,
    world: Arc<RwLock<World>>,
    /// Tells if the client accepts a file watcher that the server registers
    /// after `initialize`.
    watching: AtomicBool,
    /// Tells if the client accepts a type hierarchy registration.
    hierarchy: AtomicBool,
    /// The server increases it on each edit. Thus a debounced recompile can
    /// know that a later keystroke came before it.
    generation: Arc<AtomicU64>,
}

/// Recompiles if a change occurred. Publishes diagnostics for each file that
/// has diagnostics or had them before.
async fn publish(client: &Client, world: &RwLock<World>, focus: Option<PathBuf>, trigger: Trigger) {
    let (published, caps) = {
        let mut world = world.write().await;
        if !world.refresh(focus.as_deref()) {
            return;
        }
        let grouped = world.diagnostics_by_file();
        world.last_reported = grouped.keys().cloned().collect();

        let text_of = |path: &std::path::Path| world.text(path);
        let published = grouped
            .into_iter()
            .filter_map(|(path, diagnostics)| {
                let url = convert::path_to_url(&path)?;
                let text = world.text(&path).unwrap_or_default();
                let items = diagnostics
                    .iter()
                    .map(|diagnostic| features::to_lsp_diagnostic(diagnostic, &text, &text_of))
                    .collect::<Vec<_>>();
                // The result holds the version of the buffer that these came from. Thus a
                // client can discard them if the buffer changed after that.
                Some((url, items, world.compiled_version(&path)))
            })
            .collect::<Vec<_>>();
        (published, world.caps)
    };

    for (url, diagnostics, version) in published {
        client.publish_diagnostics(url, diagnostics, version).await;
    }
    if refreshes_after(trigger) {
        // A failure only means that the client did not accept the request.
        // The new compilation answers the next request of the client in all cases.
        if caps.semantic_refresh {
            let _ = client.semantic_tokens_refresh().await;
        }
        if caps.inlay_refresh {
            let _ = client.inlay_hint_refresh().await;
        }
        if caps.lens_refresh {
            let _ = client.code_lens_refresh().await;
        }
    }
}

impl Backend {
    fn new(client: Client) -> Backend {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Backend {
            client,
            world: Arc::new(RwLock::new(World::new(&root))),
            watching: AtomicBool::new(false),
            hierarchy: AtomicBool::new(false),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// The world, compiled with the current buffers.
    ///
    /// The server compiles edits lazily (see [`DEBOUNCE`]). Thus a request that
    /// comes while a person types compiles first. The positions in the buffer
    /// must agree with the spans in the compilation.
    async fn fresh(&self) -> tokio::sync::RwLockReadGuard<'_, World> {
        if self.world.read().await.needs_compile() {
            publish(&self.client, &self.world, None, Trigger::Edit).await;
        }
        self.world.read().await
    }

    /// Asks the client to tell the server when a `.pi` file appears, changes or
    /// disappears from the disk.
    ///
    /// No other source gives this data. `didChange` fires only for buffers that
    /// are open in the editor. Thus the server does not see a file from
    /// `piton tether` or from a script that makes files. It also does not see
    /// a branch switch or a second editor.
    /// It sees the file only when a person types in an open buffer. A new file
    /// is exactly the file that has no open buffer.
    async fn watch_sources(&self) {
        if !self.watching.load(Ordering::Relaxed) {
            return;
        }
        let pattern = format!("**/*.{}", language::EXTENSION);
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![FileSystemWatcher {
                glob_pattern: GlobPattern::String(pattern),
                // The watcher reports new, changed and deleted files. This is the
                // default, but the code states it.
                kind: Some(WatchKind::all()),
            }],
        };
        let Ok(register_options) = serde_json::to_value(options) else {
            return;
        };
        let registration = Registration {
            id: "piton-watch-sources".into(),
            method: "workspace/didChangeWatchedFiles".into(),
            register_options: Some(register_options),
        };
        if let Err(error) = self.client.register_capability(vec![registration]).await {
            self.client
                .log_message(
                    MessageType::WARNING,
                    format!("cannot watch sources for changes: {error}"),
                )
                .await;
        }
    }

    /// Tells the client that the server answers type hierarchy requests.
    ///
    /// The server registers it and does not declare it. The `ServerCapabilities`
    /// struct in this version of `lsp-types` has no `typeHierarchyProvider`
    /// field. Dynamic registration is the second method of the protocol to say
    /// the same fact. A client without support never asks. The result is the
    /// same as when the server does not advertise the feature.
    async fn offer_type_hierarchy(&self) {
        if !self.hierarchy.load(Ordering::Relaxed) {
            return;
        }
        let registration = Registration {
            id: "piton-type-hierarchy".into(),
            method: "textDocument/prepareTypeHierarchy".into(),
            register_options: None,
        };
        if let Err(error) = self.client.register_capability(vec![registration]).await {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("type hierarchy is not available in this client: {error}"),
                )
                .await;
        }
    }

    async fn source_to_output(&self, params: serde_json::Value) -> Result<serde_json::Value> {
        let world = self.fresh().await;
        Ok(features::source_to_output(&world, &params))
    }

    async fn output_to_source(&self, params: serde_json::Value) -> Result<serde_json::Value> {
        let world = self.fresh().await;
        Ok(features::output_to_source(&world, &params))
    }

    async fn preview(&self, params: serde_json::Value) -> Result<serde_json::Value> {
        let world = self.fresh().await;
        Ok(features::preview(&world, &params))
    }

    /// Recompiles if a change occurred, and publishes diagnostics again.
    async fn refresh(&self, focus: Option<PathBuf>, trigger: Trigger) {
        publish(&self.client, &self.world, focus, trigger).await;
    }

    /// The current buffer, for the requests that only need its syntax. These
    /// are format, fold and selection requests. They never wait for a compile.
    /// Thus they operate while the file is only partly written.
    async fn buffer(&self, uri: &Url) -> Option<(PathBuf, String)> {
        let path = convert::url_to_path(uri)?;
        let text = self.world.read().await.text(&path)?;
        Some((path, text))
    }

    /// Recompiles after a pause in keystrokes, if no other edit comes first.
    fn schedule(&self, focus: PathBuf) {
        let ticket = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = self.generation.clone();
        let client = self.client.clone();
        let world = self.world.clone();
        tokio::spawn(async move {
            tokio::time::sleep(DEBOUNCE).await;
            if generation.load(Ordering::SeqCst) == ticket {
                publish(&client, &world, Some(focus), Trigger::Edit).await;
            }
        });
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        {
            let mut world = self.world.write().await;
            if let Some(folder) = workspace_root(&params) {
                *world = World::new(&folder);
            }
            world.caps = client_caps(&params.capabilities);
        }

        let dynamic = params
            .capabilities
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.did_change_watched_files.as_ref())
            .and_then(|watched| watched.dynamic_registration)
            .unwrap_or(false);
        self.watching.store(dynamic, Ordering::Relaxed);
        let hierarchy = params
            .capabilities
            .text_document
            .as_ref()
            .and_then(|text| text.type_hierarchy.as_ref())
            .and_then(|hierarchy| hierarchy.dynamic_registration)
            .unwrap_or(false);
        self.hierarchy.store(hierarchy, Ordering::Relaxed);

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "piton-lsp".into(),
                version: Some(VERSION.get().copied().unwrap_or(env!("CARGO_PKG_VERSION")).into()),
            }),
            capabilities: server_capabilities(),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.watch_sources().await;
        self.offer_type_hierarchy().await;
        self.refresh(None, Trigger::Elsewhere).await;
        self.client
            .log_message(MessageType::INFO, "piton language server ready")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let Some(path) = convert::url_to_path(&params.text_document.uri) else {
            return;
        };
        {
            let mut world = self.world.write().await;
            world.open_document(
                path.clone(),
                params.text_document.text,
                params.text_document.version,
            );
        }
        self.refresh(Some(path), Trigger::Edit).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let Some(path) = convert::url_to_path(&params.text_document.uri) else {
            return;
        };
        let Some(change) = params.content_changes.into_iter().next_back() else {
            return;
        };
        {
            let mut world = self.world.write().await;
            world.open_document(path.clone(), change.text, params.text_document.version);
        }
        self.schedule(path);
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let path = convert::url_to_path(&params.text_document.uri);
        if path.as_deref().is_some_and(world::is_config_file) {
            // The configuration decides the source root, the entry, and the
            // output location. The compiler reads the saved version.
            self.world.write().await.reload_project();
        }
        // The server plans plugin builds on save, never while the user types.
        self.world.write().await.saved();
        self.refresh(path, Trigger::Elsewhere).await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut news = false;
        {
            let mut world = self.world.write().await;
            for change in &params.changes {
                let Some(path) = convert::url_to_path(&change.uri) else {
                    continue;
                };
                if path
                    .file_name()
                    .is_some_and(|name| name == language::CONFIG_FILE)
                {
                    // The configuration names the source root and the entry
                    // point. Thus a change to it changes the set of files in the
                    // project, not only the content of one file.
                    world.reload_project();
                    news = true;
                    continue;
                }
                match change.typ {
                    FileChangeType::DELETED => {
                        // A deleted file has no text for the server to use.
                        // A buffer for it hides the deletion.
                        world.close_document(&path);
                        news = true;
                    }
                    // The editor reported a change to an open file before
                    // through `didChange`. Its buffer is correct in all cases.
                    // Thus this event repeats data that the server has.
                    FileChangeType::CHANGED if world.documents.contains_key(&path) => {}
                    _ => news = true,
                }
            }
            if news {
                world.invalidate();
            }
        }
        if news {
            self.refresh(None, Trigger::Elsewhere).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let Some(path) = convert::url_to_path(&params.text_document.uri) else {
            return;
        };
        {
            let mut world = self.world.write().await;
            world.close_document(&path);
        }
        self.refresh(None, Trigger::Elsewhere).await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let world = self.fresh().await;
        Ok(features::hover(
            &world,
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let world = self.fresh().await;
        Ok(features::definition(
            &world,
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ))
    }

    async fn goto_type_definition(
        &self,
        params: request::GotoTypeDefinitionParams,
    ) -> Result<Option<request::GotoTypeDefinitionResponse>> {
        let world = self.fresh().await;
        Ok(features::type_definition(
            &world,
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let world = self.fresh().await;
        Ok(features::document_highlight(
            &world,
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ))
    }

    async fn will_rename_files(&self, params: RenameFilesParams) -> Result<Option<WorkspaceEdit>> {
        let world = self.fresh().await;
        Ok(features::will_rename_files(&world, &params.files))
    }

    async fn goto_implementation(
        &self,
        params: request::GotoImplementationParams,
    ) -> Result<Option<request::GotoImplementationResponse>> {
        let world = self.fresh().await;
        Ok(features::implementations(
            &world,
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ))
    }

    async fn prepare_type_hierarchy(
        &self,
        params: TypeHierarchyPrepareParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let world = self.fresh().await;
        Ok(features::prepare_type_hierarchy(
            &world,
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ))
    }

    async fn supertypes(
        &self,
        params: TypeHierarchySupertypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let world = self.fresh().await;
        Ok(features::type_hierarchy_supertypes(&world, &params.item))
    }

    async fn subtypes(
        &self,
        params: TypeHierarchySubtypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let world = self.fresh().await;
        Ok(features::type_hierarchy_subtypes(&world, &params.item))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let world = self.fresh().await;
        Ok(features::references(
            &world,
            &params.text_document_position.text_document.uri,
            params.text_document_position.position,
            params.context.include_declaration,
        ))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let world = self.fresh().await;
        features::prepare_rename(&world, &params.text_document.uri, params.position)
            .map_err(request_failed)
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let world = self.fresh().await;
        features::rename(
            &world,
            &params.text_document_position.text_document.uri,
            params.text_document_position.position,
            &params.new_name,
        )
        .map_err(request_failed)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let world = self.fresh().await;
        Ok(features::document_symbols(
            &world,
            &params.text_document.uri,
        ))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let world = self.fresh().await;
        Ok(features::workspace_symbols(&world, &params.query))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let world = self.fresh().await;
        Ok(features::completion(
            &world,
            &params.text_document_position.text_document.uri,
            params.text_document_position.position,
        ))
    }

    async fn completion_resolve(&self, item: CompletionItem) -> Result<CompletionItem> {
        let world = self.fresh().await;
        Ok(features::resolve_completion(&world, item))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let Some((path, text)) = self.buffer(&params.text_document.uri).await else {
            return Ok(None);
        };
        Ok(Some(features::formatting(&text, &path)))
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let Some((_, text)) = self.buffer(&params.text_document_position.text_document.uri).await
        else {
            return Ok(None);
        };
        Ok(features::on_type_formatting(
            &text,
            params.text_document_position.position,
            &params.ch,
        ))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let Some((path, text)) = self.buffer(&params.text_document.uri).await else {
            return Ok(None);
        };
        Ok(Some(features::folding(&text, &path)))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let Some((path, text)) = self.buffer(&params.text_document.uri).await else {
            return Ok(None);
        };
        Ok(Some(features::selection_ranges(&text, &path, &params.positions)))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let world = self.fresh().await;
        Ok(features::code_lenses(&world, &params.text_document.uri))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let world = self.fresh().await;
        Ok(features::code_actions_for(
            &world,
            &params.text_document.uri,
            params.range,
            params.context.only.as_deref(),
        ))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let world = self.fresh().await;
        Ok(features::inlay_hints(
            &world,
            &params.text_document.uri,
            params.range,
        ))
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        let outcome = {
            let world = self.fresh().await;
            features::execute_command(&world, &params.command, &params.arguments)
        };
        match outcome {
            Some(features::CommandOutcome::Show(show)) => {
                if let Err(error) = self.client.show_document(show).await {
                    self.client
                        .show_message(MessageType::WARNING, format!("cannot open it: {error}"))
                        .await;
                }
            }
            Some(features::CommandOutcome::Message(message)) => {
                self.client.show_message(MessageType::INFO, message).await;
            }
            None => {}
        }
        Ok(None)
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let world = self.fresh().await;
        Ok(features::semantic_tokens(&world, &params.text_document.uri))
    }
}

/// A request that the server cannot do, with the reason that the editor shows.
fn request_failed(message: String) -> tower_lsp::jsonrpc::Error {
    tower_lsp::jsonrpc::Error {
        code: tower_lsp::jsonrpc::ErrorCode::ServerError(-32803),
        message: message.into(),
        data: None,
    }
}

/// The version that the server reports to the editor. It is the version of the
/// binary, which [`serve`] takes, not the version of this crate.
static VERSION: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// Runs the language server over stdio. It reports `version` as its version.
pub fn serve(version: &'static str) {
    let _ = VERSION.set(version);
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: cannot start the language server runtime: {error}");
            return;
        }
    };
    runtime.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::build(Backend::new)
            .custom_method("piton/sourceToOutput", Backend::source_to_output)
            .custom_method("piton/outputToSource", Backend::output_to_source)
            .custom_method("piton/preview", Backend::preview)
            .finish();
        Server::new(stdin, stdout, socket).serve(service).await;
    });
}
