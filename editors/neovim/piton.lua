-- Piton for Neovim.
--
-- The spec lists Neovim's highlighter as "vim", so the Vim syntax file in
-- ../vim works as-is. This adds the tree-sitter grammar for those who prefer
-- it, and wires up the language server, which is where everything beyond
-- highlighting comes from.
--
-- Copy into your config and call `require("piton").setup()`.

local M = {}

--- Registers the tree-sitter grammar and its queries.
local function setup_treesitter(opts)
  local ok, parsers = pcall(require, "nvim-treesitter.parsers")
  if not ok then
    return
  end
  parsers.get_parser_configs().piton = {
    install_info = {
      url = opts.grammar_url or "https://github.com/mctavishdynamics/piton",
      location = "editors/tree-sitter-piton",
      files = { "src/parser.c" },
      branch = "main",
    },
    filetype = "piton",
  }
end

--- Starts `piton lsp` for Piton buffers.
local function setup_lsp(opts)
  local ok, lspconfig = pcall(require, "lspconfig")
  if not ok then
    return
  end
  local configs = require("lspconfig.configs")
  if not configs.piton then
    configs.piton = {
      default_config = {
        cmd = { opts.command or "piton", "lsp" },
        filetypes = { "piton" },
        -- The project configuration decides what is reachable, so it is the
        -- root marker.
        root_dir = lspconfig.util.root_pattern("piton.config.pi", ".git"),
        single_file_support = true,
        settings = {},
      },
    }
  end
  lspconfig.piton.setup(opts.server or {})
end

function M.setup(opts)
  opts = opts or {}

  vim.filetype.add({ extension = { pi = "piton" } })

  vim.api.nvim_create_autocmd("FileType", {
    pattern = "piton",
    callback = function()
      -- Four spaces, not tabs, and not configurable.
      vim.bo.expandtab = true
      vim.bo.shiftwidth = 4
      vim.bo.softtabstop = 4
      vim.bo.tabstop = 4
      vim.bo.commentstring = "// %s"
      vim.bo.formatprg = "piton format /dev/stdin"
    end,
  })

  setup_treesitter(opts)
  setup_lsp(opts)
end

return M
