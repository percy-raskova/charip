# charip-lsp

A Language Server for MyST (Markedly Structured Text) documents, forked from [markdown-oxide](https://github.com/Feel-ix-343/markdown-oxide).

## About the Name

**Charip** (자립, *ja-rip*) means "self-reliance" or "economic self-sufficiency" in Korean. It is one of the three pillars of the Juche idea, which holds that true independence requires building one's own infrastructure rather than depending on external systems.

This project embodies that principle: rather than adapting to tools built for other purposes, we build tools that serve our specific needs. Revolutionary movements need revolutionary infrastructure—this LSP is one component of that effort, providing intelligence for a personal knowledge system that publishes revolutionary ideas.

## Purpose

**charip-lsp** extends markdown-oxide to support **MyST Markdown**, the extended Markdown syntax used by Sphinx documentation systems. While markdown-oxide targets Obsidian/Logseq PKM workflows with wikilinks, charip-lsp targets:

- MyST directives (```{note}, ```{warning}, ```{toctree})
- MyST roles ({ref}\`target\`, {doc}\`path\`, {term}\`entry\`)
- Cross-reference resolution across document trees
- Sphinx project structure (toctrees, includes)

The immediate use case is providing LSP features for a personal knowledge vault that publishes to a Sphinx website.

## Status

**Phase 1 Complete**: Basic MyST directive and anchor parsing integrated into the vault.

**Phase 2 In Progress**: Graph-based architecture using `petgraph` for cross-file reference resolution.

See `ai-docs/MyST_Implementation_Plan.md` for the full roadmap.

---

# Upstream Documentation (markdown-oxide)

The following documentation is from the upstream project and applies to the base PKM features.

**[Quick Start](#quick-start)**

## Recommended Links

* [What is markdown-oxide?](https://oxide.md/): An overview of the PKM features
* [Markdown-oxide getting-started guide](https://oxide.md/index#Getting+Started): Editor setup and configuration
* [Features Reference](https://oxide.md/Features+Index): All features
* [Configuration Reference](https://oxide.md/Configuration): Configuration options
    + [Default Config File](https://oxide.md/Configuration#Default+Config+File)

# Quick Start

Get started with Markdown-oxide as fast as possible! (Mac + Linux)

Set up the PKM for your text editor...

- [Neovim](#Neovim)
- [VSCode](#VSCode)
- [Zed](#Zed)
- [Helix](#Helix)

## Neovim

- Give Neovim access to the binary.

    - <details>
         <summary>Cargo Install (from source)</summary>
    
        ```bash
        cargo install --locked --git https://github.com/Feel-ix-343/markdown-oxide.git markdown-oxide
        ```
    
    </details>

    - <details>
         <summary>Cargo binstall (from hosted binary)</summary>
    
        ```bash
        cargo binstall --git 'https://github.com/feel-ix-343/markdown-oxide' markdown-oxide
        ```
    
    </details>
    
    - Arch Linux: `pacman -S markdown-oxide`
    - [Mason.nvim](https://github.com/williamboman/mason.nvim) (from hosted binary)
    - Nix: `pkgs.markdown-oxide`
    - Alpine Linux: `apk add markdown-oxide`
    - openSUSE: `zypper install markdown-oxide`
    - Conda: `conda install conda-forge::markdown-oxide`
    
    - <details>
         <summary>Winget (Windows)</summary>
    
        ```bash
        winget install FelixZeller.markdown-oxide
        ```
    
    </details>
    
    - <details>
         <summary>Homebrew (from package manager)</summary>
    
        ```bash
        brew install markdown-oxide
        ```
    
    </details>
  
- Modify your Neovim Configuration ^nvimconfigsetup
    - <details>
        <summary>Modify LSP Config (making sure to adjust capabilities as follows)</summary>

        ```lua        
        -- An example nvim-lspconfig capabilities setting
        local capabilities = require("cmp_nvim_lsp").default_capabilities(vim.lsp.protocol.make_client_capabilities())
        
        require("lspconfig").markdown_oxide.setup({
            -- Ensure that dynamicRegistration is enabled! This allows the LS to take into account actions like the
            -- Create Unresolved File code action, resolving completions for unindexed code blocks, ...
            capabilities = vim.tbl_deep_extend(
                'force',
                capabilities,
                {
                    workspace = {
                        didChangeWatchedFiles = {
                            dynamicRegistration = true,
                        },
                    },
                }
            ),
            on_attach = on_attach -- configure your on attach config
        })
        ```

    </details> 

    - <details>
        <summary>Modify your nvim-cmp configuration</summary>

        Modify your nvim-cmp source settings for nvim-lsp (note: you must have nvim-lsp installed)

        ```lua        
        {
        name = 'nvim_lsp',
          option = {
            markdown_oxide = {
              keyword_pattern = [[\(\k\| \|\/\|#\)\+]]
            }
          }
        },
        ```

    </details>

    - <details>
        <summary>(optional) Enable Code Lens (eg for UI reference count)</summary>

        Modify your lsp `on_attach` function.

        ```lua
        local function codelens_supported(bufnr)
          for _, c in ipairs(vim.lsp.get_clients({ bufnr = bufnr })) do
            if c.server_capabilities and c.server_capabilities.codeLensProvider then
              return true
            end
          end
          return false
        end

        vim.api.nvim_create_autocmd(
          { 'TextChanged', 'InsertLeave', 'CursorHold', 'BufEnter' },
          {
            buffer = bufnr,
            callback = function()
              if codelens_supported(bufnr) then
                vim.lsp.codelens.refresh({ bufnr = bufnr })
              end
            end,
          }
        )

        if codelens_supported(bufnr) then
          vim.lsp.codelens.refresh({ bufnr = bufnr })
        end
        ```

    </details>

    - <details>
        <summary>(optional) Enable opening daily notes with natural language</summary>

        Modify your lsp `on_attach` function to support opening daily notes with natural language and relative directives.

        Examples:
        - Natural language: `:Daily two days ago`, `:Daily next monday`
        - Relative directives: `:Daily prev`, `:Daily next`, `:Daily +7`, `:Daily -3`

        ```lua
        -- setup Markdown Oxide daily note commands
        if client.name == "markdown_oxide" then

          vim.api.nvim_create_user_command(
            "Daily",
            function(args)
              local input = args.args

              vim.lsp.buf.execute_command({command="jump", arguments={input}})

            end,
            {desc = 'Open daily note', nargs = "*"}
          )
        end
        ```

    </details>    
- Ensure relevant plugins are installed:
    * [Nvim CMP](https://github.com/hrsh7th/nvim-cmp): UI for using LSP completions
    * [Telescope](https://github.com/nvim-telescope/telescope.nvim): UI helpful for the LSP references implementation
        - Allows you to view and fuzzy match backlinks to files, headings, and blocks.
    * [Lspsaga](https://github.com/nvimdev/lspsaga.nvim): UI generally helpful for LSP commands
        + Allows you to edit linked markdown files in a popup window, for example. 


## VSCode

Install the [vscode extension](https://marketplace.visualstudio.com/items?itemName=FelixZeller.markdown-oxide) (called `Markdown Oxide`). As for how the extension uses the language server, there are two options
- Recommended: the extension will download the server's binary and use that
- The extension will use `markdown-oxide` from path. To install to your path, there are the following methods for VSCode:

    - <details>
         <summary>Cargo Install (from source)</summary>
    
        ```bash
        cargo install --locked --git https://github.com/Feel-ix-343/markdown-oxide.git markdown-oxide
        ```
    
    </details>

    - <details>
         <summary>Cargo binstall[1] (from hosted binary)</summary>
    
        ```bash
        cargo binstall --git 'https://github.com/feel-ix-343/markdown-oxide' markdown-oxide
        ```
    
    </details>
    
    - Arch Linux: `pacman -S markdown-oxide`
    - Nix: `pkgs.markdown-oxide`
    - Alpine Linux: `apk add markdown-oxide`
    - openSUSE: `zypper install markdown-oxide`
    - Conda: `conda install conda-forge::markdown-oxide`
    
    - <details>
         <summary>Winget (Windows)</summary>
    
        ```bash
        winget install FelixZeller.markdown-oxide
        ```
    
    </details>
    
    - <details>
         <summary>Homebrew (from package manager)</summary>
    
        ```bash
        brew install markdown-oxide
        ```
    
    </details>

## Zed

Markdown Oxide is available as an extension titled `Markdown Oxide`. Similarly to VSCode, there are two methods for this extension to access the language server
- Recommended: the extension will download the server's binary and use that
- The extension will use `markdown-oxide` from path. To install to your path, there are the following methods for Zed:

    - <details>
         <summary>Cargo Install (from source)</summary>
    
        ```bash
        cargo install --locked --git https://github.com/Feel-ix-343/markdown-oxide.git markdown-oxide
        ```
    
    </details>

    - <details>
         <summary>Cargo binstall[1] (from hosted binary)</summary>
    
        ```bash
        cargo binstall --git 'https://github.com/feel-ix-343/markdown-oxide' markdown-oxide
        ```
    
    </details>
    
    - Arch Linux: `pacman -S markdown-oxide`
    - Nix: `pkgs.markdown-oxide`
    - Alpine Linux: `apk add markdown-oxide`
    - openSUSE: `zypper install markdown-oxide`
    - Conda: `conda install conda-forge::markdown-oxide`
    
    - <details>
         <summary>Winget (Windows)</summary>
    
        ```bash
        winget install FelixZeller.markdown-oxide
        ```
    
    </details>
    
    - <details>
         <summary>Homebrew (from package manager)</summary>
    
        ```bash
        brew install markdown-oxide
        ```
    
    </details>

    

## Helix

For Helix, all you must do is install the language server's binary to your path. The following installation methods are available:
- <details>
     <summary>Cargo Install (from source)</summary>

    ```bash
    cargo install --locked --git https://github.com/Feel-ix-343/markdown-oxide.git markdown-oxide
    ```

</details>

- <details>
    <summary>Cargo binstall[1] (from hosted binary)</summary>
    
    ```bash
    cargo binstall --git 'https://github.com/feel-ix-343/markdown-oxide' markdown-oxide
    ```
    
</details>

- Arch Linux: `pacman -S markdown-oxide`
- Nix: `pkgs.markdown-oxide`
- Alpine Linux: `apk add markdown-oxide`
- openSUSE: `zypper install markdown-oxide`
- Conda: `conda install conda-forge::markdown-oxide`

- <details>
     <summary>Winget (Windows)</summary>

    ```bash
    winget install FelixZeller.markdown-oxide
    ```

</details>

- <details>
     <summary>Homebrew (from package manager)</summary>

    ```bash
    brew install markdown-oxide
    ```

</details>
