-- Piton for Neovim.
--
-- The spec lists Neovim's highlighter as "vim", so highlighting and the two
-- Enter rules come from the Vim runtime files in ../vim (syntax/, indent/,
-- ftplugin/). setup() puts that directory on 'runtimepath' so they load with no
-- further configuration. The language server supplies everything beyond
-- highlighting. The tree-sitter grammar is registered too, for anyone who
-- prefers it, but it is not needed.
--
-- Put this file on your 'runtimepath' as lua/piton.lua (or keep the repository
-- checkout and add editors/neovim to package.path) and call
-- `require("piton").setup()`. If the file is copied somewhere that has no
-- sibling ../vim directory, pass its location: `setup({ vim_runtime = ... })`.

local M = {}

--- The published grammar, the same repository and revision the Zed extension
--- pins in editors/zed/extension.toml. `cargo xtask publish-grammar` updates
--- both.
local GRAMMAR_URL = "https://github.com/piton-lang/tree-sitter-piton"
local GRAMMAR_REVISION = "5d2fa75e8776694e316c6505bc8ba646121e56b8"

--- Where the Vim runtime files live: an explicit option, else ../vim beside
--- this file (editors/neovim/piton.lua -> editors/vim).
local function vim_runtime(opts)
  if opts.vim_runtime then
    return vim.fn.fnamemodify(opts.vim_runtime, ":p")
  end
  local source = debug.getinfo(1, "S").source
  if source:sub(1, 1) ~= "@" then
    return nil
  end
  local here = vim.fn.fnamemodify(source:sub(2), ":p:h")
  -- Both editors/neovim/piton.lua and editors/neovim/lua/piton.lua layouts.
  for _, candidate in ipairs({ here .. "/../vim", here .. "/../../vim" }) do
    local resolved = vim.fn.fnamemodify(candidate, ":p")
    if vim.fn.isdirectory(resolved .. "/syntax") == 1 then
      return resolved
    end
  end
  return nil
end

--- Loads syntax/piton.vim, indent/piton.vim and ftplugin/piton.vim.
local function setup_vim_runtime(opts)
  local dir = vim_runtime(opts)
  if not dir then
    vim.notify(
      "piton: cannot find the Vim runtime files (editors/vim); pass setup({ vim_runtime = ... }) "
        .. "for highlighting and the Enter rules",
      vim.log.levels.WARN
    )
    return
  end
  dir = dir:gsub("/$", "")
  if not vim.tbl_contains(vim.opt.runtimepath:get(), dir) then
    vim.opt.runtimepath:prepend(dir)
  end
end

--- Registers the tree-sitter grammar with nvim-treesitter, when it is present.
---
--- Both nvim-treesitter branches are handled: `master` reads
--- get_parser_configs(), `main` reads the parsers table on the TSUpdate event.
--- Enabling tree-sitter highlighting or indentation for Piton is left to the
--- user; indentation in particular should stay with indent/piton.vim, because
--- a tree-sitter indent query cannot express the blank-line dedent.
local function setup_treesitter(opts)
  local ok, parsers = pcall(require, "nvim-treesitter.parsers")
  if not ok then
    return
  end
  local install_info = {
    url = opts.grammar_url or GRAMMAR_URL,
    revision = opts.grammar_revision or GRAMMAR_REVISION,
    files = { "src/parser.c" },
    branch = "main",
    queries = "queries",
  }
  if type(parsers.get_parser_configs) == "function" then
    parsers.get_parser_configs().piton = { install_info = install_info, filetype = "piton" }
  else
    vim.api.nvim_create_autocmd("User", {
      pattern = "TSUpdate",
      callback = function()
        require("nvim-treesitter.parsers").piton = { install_info = install_info }
      end,
    })
  end
end

--- Starts `piton lsp` for Piton buffers.
---
--- Neovim 0.11+ has vim.lsp.config/vim.lsp.enable built in; older versions go
--- through nvim-lspconfig.
local function setup_lsp(opts)
  local cmd = { opts.command or "piton", "lsp" }
  -- The project configuration decides what is reachable, so it is the root
  -- marker.
  local markers = { "piton.config.pi", ".git" }

  if vim.lsp.config and vim.lsp.enable then
    vim.lsp.config("piton", vim.tbl_deep_extend("force", {
      cmd = cmd,
      filetypes = { "piton" },
      root_markers = markers,
    }, opts.server or {}))
    vim.lsp.enable("piton")
    return
  end

  local ok, lspconfig = pcall(require, "lspconfig")
  if not ok then
    return
  end
  local configs = require("lspconfig.configs")
  if not configs.piton then
    configs.piton = {
      default_config = {
        cmd = cmd,
        filetypes = { "piton" },
        root_dir = lspconfig.util.root_pattern(unpack(markers)),
        single_file_support = true,
        settings = {},
      },
    }
  end
  lspconfig.piton.setup(opts.server or {})
end

--- Lets the server answer Enter through textDocument/onTypeFormatting, where
--- this Neovim can ask it to (0.12+). indent/piton.vim already implements both
--- Enter rules, and the server computes the same indentation, so this is a
--- second source of the same answer rather than a replacement.
local function setup_on_type_formatting(opts)
  if opts.on_type_formatting == false then
    return
  end
  local otf = vim.lsp.on_type_formatting
  if type(otf) ~= "table" or type(otf.enable) ~= "function" then
    return
  end
  vim.api.nvim_create_autocmd("LspAttach", {
    callback = function(args)
      local client = vim.lsp.get_client_by_id(args.data.client_id)
      if not client or client.name ~= "piton" then
        return
      end
      pcall(otf.enable, true, { client_id = client.id })
    end,
  })
end

--- Formats a Piton buffer before it is saved, when the option is on.
---
--- autoFormatOnSave is an option: the autocmd is only registered when setup()
--- is given { format_on_save = true }. Formatting runs through the language
--- server, which adds the space after `//` and touches nothing else that is
--- commented.
local function setup_format_on_save(opts)
  if not opts.format_on_save then
    return
  end

  vim.api.nvim_create_autocmd("BufWritePre", {
    pattern = "*.pi",
    callback = function(args)
      local clients = vim.lsp.get_clients({
        bufnr = args.buf,
        method = "textDocument/formatting",
      })
      if #clients > 0 then
        vim.lsp.buf.format({ bufnr = args.buf, async = false })
      end
    end,
  })
end

function M.setup(opts)
  opts = opts or {}

  vim.filetype.add({ extension = { pi = "piton" } })
  setup_vim_runtime(opts)

  vim.api.nvim_create_autocmd("FileType", {
    pattern = "piton",
    callback = function()
      -- Four spaces, not tabs, and not configurable. ftplugin/piton.vim sets
      -- the same; repeated here in case the Vim runtime was not found.
      vim.bo.expandtab = true
      vim.bo.shiftwidth = 4
      vim.bo.softtabstop = 4
      vim.bo.tabstop = 4
      vim.bo.commentstring = "// %s"
      -- `-` reads stdin and writes the formatted text to stdout.
      vim.bo.formatprg = "piton format -"
    end,
  })

  setup_treesitter(opts)
  setup_lsp(opts)
  setup_on_type_formatting(opts)
  setup_format_on_save(opts)
end

return M
