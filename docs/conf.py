# Configuration file for the Sphinx documentation builder.
# charip-lsp documentation

# -- Project information -----------------------------------------------------
project = 'charip-lsp'
copyright = '2024-2025'
author = 'charip-lsp contributors'

# -- General configuration ---------------------------------------------------
extensions = [
    'myst_parser',
    'sphinx.ext.autodoc',
    'sphinx.ext.intersphinx',
]

# MyST extensions (mirrors rstnotes target environment)
myst_enable_extensions = [
    'colon_fence',      # ::: directive syntax
    'deflist',          # Definition lists
    'dollarmath',       # $math$ syntax
    'fieldlist',        # :field: value syntax
    'substitution',     # {{variable}} syntax
    'tasklist',         # [ ] checkboxes
    'attrs_inline',     # {#id .class} attributes
]

# Substitutions for documentation
myst_substitutions = {
    'project': 'charip-lsp',
    'upstream': 'markdown-oxide',
}

# Source file suffixes
source_suffix = {
    '.rst': 'restructuredtext',
    '.md': 'markdown',
}

# The master toctree document
master_doc = 'index'

# Exclude patterns - don't scan these directories
exclude_patterns = [
    '_build',
    '.venv',
    'Markdown Oxide Docs',  # Upstream docs, kept for reference
    'README.md',            # Project root README, not part of sphinx docs
    '**/.DS_Store',
]

# -- Options for HTML output -------------------------------------------------
html_theme = 'furo'  # Clean, modern theme
html_title = 'charip-lsp'
html_static_path = ['_static']
html_css_files = ['custom.css']

# Furo theme options
html_theme_options = {
    "dark_css_variables": {
        "color-background-primary": "#024fa2",
        "color-background-secondary": "#013d7a",
        "color-brand-primary": "#ed1c27",
        "color-brand-content": "#ff4d4d",
    },
    "light_css_variables": {
        "color-background-primary": "#024fa2",
        "color-background-secondary": "#013d7a",
        "color-brand-primary": "#ed1c27",
        "color-brand-content": "#ff4d4d",
    },
}

# -- Intersphinx configuration -----------------------------------------------
intersphinx_mapping = {
    'python': ('https://docs.python.org/3', None),
    'sphinx': ('https://www.sphinx-doc.org/en/master/', None),
}
