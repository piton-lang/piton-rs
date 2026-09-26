//! The Piton language server.
//!
//! Every feature here answers a question about the *resolved* program rather
//! than about raw syntax: hovering a user keyword finds the anchor it aliases,
//! going to the definition of an inherited property lands on the base that
//! declares it, and renaming a symbol reaches the imports that carry it.

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

/// Semantic token types, in the order the client is told about them.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::CLASS,    // anchors
    SemanticTokenType::PROPERTY, // properties
    SemanticTokenType::VARIABLE, // variables
    SemanticTokenType::KEYWORD,  // language and user keywords
    SemanticTokenType::COMMENT,
    SemanticTokenType::STRING, // prose
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::NAMESPACE, // module paths
    SemanticTokenType::TYPE,      // type constraints
];

/// Semantic token modifiers. The last three are Piton's own: a property whose
/// value a base supplies, a declaration other files can import, and a name
/// declared in another file.
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::ABSTRACT,
    SemanticTokenModifier::DEFAULT_LIBRARY,
    SemanticTokenModifier::new("inherited"),
    SemanticTokenModifier::new("exported"),
    SemanticTokenModifier::new("imported"),
];

/// How long typing has to pause before the server recompiles on its own.
///
/// A request (hover, completion, ...) never waits for this: it compiles first
/// if the buffers moved since the last compilation. The pause only decides
/// when diagnostics are republished while someone is still typing.
const DEBOUNCE: Duration = Duration::from_millis(250);

pub struct Backend {
    client: Client,
    world: Arc<RwLock<World>>,
    /// Whether the client said it would accept a file watcher registered after
    /// `initialize`.
    watching: AtomicBool,
    /// Whether the client can take a type hierarchy registration.
    hierarchy: AtomicBool,
    /// Bumped on every edit, so a debounced recompile can tell it was
    /// overtaken by a later keystroke.
    generation: Arc<AtomicU64>,
}

/// Recompiles if anything changed and publishes diagnostics for every file
/// that has or had them.
async fn publish(client: &Client, world: &RwLock<World>, focus: Option<PathBuf>) {
    let published = {
        let mut world = world.write().await;
        if !world.refresh(focus.as_deref()) {
            return;
        }
        let grouped = world.diagnostics_by_file();
        world.last_reported = grouped.keys().cloned().collect();

        let text_of = |path: &std::path::Path| world.text(path);
        grouped
            .into_iter()
            .filter_map(|(path, diagnostics)| {
                let url = convert::path_to_url(&path)?;
                let text = world.text(&path).unwrap_or_default();
                let items = diagnostics
                    .iter()
                    .map(|diagnostic| features::to_lsp_diagnostic(diagnostic, &text, &text_of))
                    .collect::<Vec<_>>();
                Some((url, items))
            })
            .collect::<Vec<_>>()
    };

    for (url, diagnostics) in published {
        client.publish_diagnostics(url, diagnostics, None).await;
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

    /// The world, compiled against the current buffers.
    ///
    /// Edits are compiled lazily (see [`DEBOUNCE`]), so a request that arrives
    /// mid-typing compiles first -- positions in the buffer have to agree with
    /// the spans in the compilation.
    async fn fresh(&self) -> tokio::sync::RwLockReadGuard<'_, World> {
        if self.world.read().await.needs_compile() {
            publish(&self.client, &self.world, None).await;
        }
        self.world.read().await
    }

    /// Asks the client to tell the server when a `.pi` file appears, changes or
    /// goes away on disk.
    ///
    /// Nothing else will say so. `didChange` fires only for buffers the editor
    /// has open, so a file written by `piton tether`, a scaffolding script, a
    /// branch switch or a second editor is invisible until someone happens to
    /// type in an open buffer -- and a file that has just been created is
    /// exactly the file with nothing open in it.
    async fn watch_sources(&self) {
        if !self.watching.load(Ordering::Relaxed) {
            return;
        }
        let pattern = format!("**/*.{}", language::EXTENSION);
        let options = DidChangeWatchedFilesRegistrationOptions {
            watchers: vec![FileSystemWatcher {
                glob_pattern: GlobPattern::String(pattern),
                // Created, changed and deleted: the default, said out loud.
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

    /// Tells the client the server answers type hierarchy requests.
    ///
    /// Registered rather than declared: the `ServerCapabilities` struct in this
    /// version of `lsp-types` has no `typeHierarchyProvider` field, and dynamic
    /// registration is the protocol's own second way of saying the same thing.
    /// A client that does not support it simply never asks, which is the same
    /// outcome as not advertising the feature.
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

    /// Recompiles, if anything changed, and republishes diagnostics.
    async fn refresh(&self, focus: Option<PathBuf>) {
        publish(&self.client, &self.world, focus).await;
    }

    /// Recompiles after typing pauses, unless another edit arrives first.
    fn schedule(&self, focus: PathBuf) {
        let ticket = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = self.generation.clone();
        let client = self.client.clone();
        let world = self.world.clone();
        tokio::spawn(async move {
            tokio::time::sleep(DEBOUNCE).await;
            if generation.load(Ordering::SeqCst) == ticket {
                publish(&client, &world, Some(focus)).await;
            }
        });
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(folder) = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|folder| convert::url_to_path(&folder.uri))
        {
            let mut world = self.world.write().await;
            *world = World::new(&folder);
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
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
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
                    // The list is built on every keystroke and the description
                    // of an anchor renders its whole compiled value, so the
                    // description is built later, for the one item the cursor
                    // settles on.
                    resolve_provider: Some(true),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
                // Pressing enter after a line that ends in a colon should land
                // inside the block it opened. The grammar cannot say that --
                // indentation is not in it -- so the server does.
                document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                    first_trigger_character: "\n".into(),
                    more_trigger_character: None,
                }),
                folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
                selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![
                            CodeActionKind::QUICKFIX,
                            CodeActionKind::REFACTOR,
                            CodeActionKind::SOURCE,
                            CodeActionKind::SOURCE_ORGANIZE_IMPORTS,
                        ]),
                        work_done_progress_options: Default::default(),
                        resolve_provider: Some(false),
                    },
                )),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: Default::default(),
                }),
                inlay_hint_provider: Some(OneOf::Left(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        features::SOURCE_TO_OUTPUT.into(),
                        features::SHOW_LOCATION.into(),
                    ],
                    work_done_progress_options: Default::default(),
                }),
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            legend: SemanticTokensLegend {
                                token_types: TOKEN_TYPES.to_vec(),
                                token_modifiers: TOKEN_MODIFIERS.to_vec(),
                            },
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                            ..Default::default()
                        },
                    ),
                ),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.watch_sources().await;
        self.offer_type_hierarchy().await;
        self.refresh(None).await;
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
            world.set_document(path.clone(), params.text_document.text);
        }
        self.refresh(Some(path)).await;
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
            world.set_document(path.clone(), change.text);
        }
        self.schedule(path);
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let path = convert::url_to_path(&params.text_document.uri);
        if path.as_deref().is_some_and(world::is_config_file) {
            // The configuration decides the source root, the entry, and where
            // output goes; the saved version is the one the compiler reads.
            self.world.write().await.reload_project();
        }
        self.refresh(path).await;
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
                    // point, so a change to it changes which files are the
                    // project at all, not just what one of them says.
                    world.reload_project();
                    news = true;
                    continue;
                }
                match change.typ {
                    FileChangeType::DELETED => {
                        // A deleted file has no text left to fall back to, and
                        // a buffer kept for it would hide the deletion.
                        world.close_document(&path);
                        news = true;
                    }
                    // A file the editor has open was already reported through
                    // `didChange`, and its buffer is the truth in any case, so
                    // this is a second telling of something already heard.
                    FileChangeType::CHANGED if world.documents.contains_key(&path) => {}
                    _ => news = true,
                }
            }
            if news {
                world.invalidate();
            }
        }
        if news {
            self.refresh(None).await;
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
        self.refresh(None).await;
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
        Ok(features::prepare_rename(
            &world,
            &params.text_document.uri,
            params.position,
        ))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let world = self.fresh().await;
        Ok(features::rename(
            &world,
            &params.text_document_position.text_document.uri,
            params.text_document_position.position,
            &params.new_name,
        ))
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
        let world = self.fresh().await;
        Ok(features::formatting(&world, &params.text_document.uri))
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let world = self.fresh().await;
        Ok(features::on_type_formatting(
            &world,
            &params.text_document_position.text_document.uri,
            params.text_document_position.position,
            &params.ch,
        ))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let world = self.fresh().await;
        Ok(features::folding(&world, &params.text_document.uri))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let world = self.fresh().await;
        Ok(features::selection_ranges(
            &world,
            &params.text_document.uri,
            &params.positions,
        ))
    }

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let world = self.fresh().await;
        Ok(features::code_lenses(&world, &params.text_document.uri))
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let world = self.fresh().await;
        Ok(features::document_links(&world, &params.text_document.uri))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let world = self.fresh().await;
        Ok(features::code_actions(
            &world,
            &params.text_document.uri,
            params.range,
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

/// Runs the language server over stdio.
/// The version the server reports to the editor: the binary's, which
/// [`serve`] is given, rather than this crate's.
static VERSION: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();

/// Runs the language server over stdio, reporting `version` as its own.
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
