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
}

impl Backend {
    fn new(client: Client) -> Backend {
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Backend {
            client,
            world: RwLock::new(World::new(&root)),
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
                        " ".into(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                document_symbol_provider: Some(OneOf::Left(true)),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                document_formatting_provider: Some(OneOf::Left(true)),
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

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let world = self.world.read().await;
        Ok(features::formatting(&world, &params.text_document.uri))
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
