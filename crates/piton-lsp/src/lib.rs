//! The Piton language server.
//!
//! Every feature here answers a question about the *resolved* program rather
//! than about raw syntax: hovering a user keyword finds the anchor it aliases,
//! going to the definition of an inherited property lands on the base that
//! declares it, and renaming a symbol reaches the imports that carry it.

pub mod convert;
pub mod features;
pub mod index;
pub mod world;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use piton_syntax::language;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use world::World;

/// Semantic token types, in the order the client is told about them.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::CLASS,     // anchors
    SemanticTokenType::PROPERTY,  // properties
    SemanticTokenType::VARIABLE,  // variables
    SemanticTokenType::KEYWORD,   // language and user keywords
    SemanticTokenType::COMMENT,
    SemanticTokenType::STRING,    // prose
    SemanticTokenType::NUMBER,
    SemanticTokenType::OPERATOR,
    SemanticTokenType::NAMESPACE, // module paths
    SemanticTokenType::TYPE,      // type constraints
];

/// Semantic token modifiers.
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,
    SemanticTokenModifier::DEFINITION,
    SemanticTokenModifier::ABSTRACT,
    SemanticTokenModifier::DEFAULT_LIBRARY,
];

pub struct Backend {
    client: Client,
    world: RwLock<World>,
    /// Whether the client said it would accept a file watcher registered after
    /// `initialize`.
    watching: AtomicBool,
}

impl Backend {
    fn new(client: Client) -> Backend {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Backend {
            client,
            world: RwLock::new(World::new(&root)),
            watching: AtomicBool::new(false),
        }
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

    /// Recompiles and republishes diagnostics for every affected file.
    async fn refresh(&self, focus: Option<PathBuf>) {
        let published = {
            let mut world = self.world.write().await;
            world.recompile(focus.as_deref());
            let grouped = world.diagnostics_by_file();
            world.last_reported = grouped.keys().cloned().collect();

            grouped
                .into_iter()
                .filter_map(|(path, diagnostics)| {
                    let url = convert::path_to_url(&path)?;
                    let text = world.text(&path).unwrap_or_default();
                    let items = diagnostics
                        .iter()
                        .map(|diagnostic| features::to_lsp_diagnostic(diagnostic, &text))
                        .collect::<Vec<_>>();
                    Some((url, items))
                })
                .collect::<Vec<_>>()
        };

        for (url, diagnostics) in published {
            self.client.publish_diagnostics(url, diagnostics, None).await;
        }
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

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "piton-lsp".into(),
                version: Some(env!("CARGO_PKG_VERSION").into()),
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
                        " ".into(),
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
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                signature_help_provider: Some(SignatureHelpOptions {
                    trigger_characters: Some(vec![":".into()]),
                    ..Default::default()
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
        self.refresh(Some(path)).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let path = convert::url_to_path(&params.text_document.uri);
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
                if path.file_name().is_some_and(|name| name == language::CONFIG_FILE) {
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
        let world = self.world.read().await;
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
        let world = self.world.read().await;
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
        let world = self.world.read().await;
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
        let world = self.world.read().await;
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
        let world = self.world.read().await;
        Ok(features::type_hierarchy_supertypes(&world, &params.item))
    }

    async fn subtypes(
        &self,
        params: TypeHierarchySubtypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let world = self.world.read().await;
        Ok(features::type_hierarchy_subtypes(&world, &params.item))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let world = self.world.read().await;
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
        let world = self.world.read().await;
        Ok(features::prepare_rename(
            &world,
            &params.text_document.uri,
            params.position,
        ))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let world = self.world.read().await;
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
        let world = self.world.read().await;
        Ok(features::document_symbols(&world, &params.text_document.uri))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let world = self.world.read().await;
        Ok(features::workspace_symbols(&world, &params.query))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let world = self.world.read().await;
        Ok(features::completion(
            &world,
            &params.text_document_position.text_document.uri,
            params.text_document_position.position,
        ))
    }

    async fn completion_resolve(&self, item: CompletionItem) -> Result<CompletionItem> {
        let world = self.world.read().await;
        Ok(features::resolve_completion(&world, item))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let world = self.world.read().await;
        Ok(features::formatting(&world, &params.text_document.uri))
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let world = self.world.read().await;
        Ok(features::on_type_formatting(
            &world,
            &params.text_document_position.text_document.uri,
            params.text_document_position.position,
            &params.ch,
        ))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let world = self.world.read().await;
        Ok(features::folding(&world, &params.text_document.uri))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let world = self.world.read().await;
        Ok(features::selection_ranges(
            &world,
            &params.text_document.uri,
            &params.positions,
        ))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let world = self.world.read().await;
        Ok(features::code_actions(
            &world,
            &params.text_document.uri,
            params.range,
        ))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let world = self.world.read().await;
        Ok(features::inlay_hints(
            &world,
            &params.text_document.uri,
            params.range,
        ))
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let world = self.world.read().await;
        Ok(features::signature_help(
            &world,
            &params.text_document_position_params.text_document.uri,
            params.text_document_position_params.position,
        ))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let world = self.world.read().await;
        Ok(features::semantic_tokens(&world, &params.text_document.uri))
    }
}

/// Runs the language server over stdio.
pub fn serve() {
    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("error: cannot start the language server runtime: {error}");
            return;
        }
    };
    runtime.block_on(async {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(Backend::new);
        Server::new(stdin, stdout, socket).serve(service).await;
    });
}
