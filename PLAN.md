

# **Architectural Roadmap for a Rust-Based MyST Language Server: Extending Markdown-Oxide for the Neovim Ecosystem**

## **1\. Executive Summary**

The domain of technical documentation has recently witnessed a paradigm shift with the emergence of Markedly Structured Text (MyST). By bridging the syntactic accessibility of CommonMark with the semantic rigor of reStructuredText (rST), MyST has become the lingua franca for the Sphinx ecosystem and scientific publishing. However, the tooling landscape for MyST remains fragmented. While the Python ecosystem offers robust build-time tools via myst-parser and sphinx, the developer experience (DX) within high-performance, modal editors like Neovim lacks the sophistication of a dedicated Language Server Protocol (LSP) implementation. The current reliance on generic Markdown servers often leaves developers without critical features such as directive autocompletion, cross-reference resolution, and real-time diagnostic linting for complex document trees.  
This report presents an exhaustive technical roadmap for engineering a custom, high-performance MyST LSP. The proposed solution rejects the path of wrapping existing Python tooling in favor of a Rust-native implementation. Specifically, it advocates for extending the architecture of markdown-oxide, a Personal Knowledge Management (PKM) tool, to support the hierarchical and directed acyclic graph (DAG) structures inherent in technical documentation. This approach leverages the raw performance of Rust and the graph capabilities of petgraph to deliver millisecond-latency intelligence, even in workspaces containing thousands of interlinked documents.  
The roadmap delineates a phased execution strategy. First, it necessitates the replacement of the standard CommonMark parser with markdown-rs to enable Abstract Syntax Tree (AST) manipulation for MyST directives.1 Second, it details the transformation of markdown-oxide's flat graph architecture into a directed graph capable of modeling toctree hierarchies and include transclusions.2 Finally, it prescribes a deep integration strategy for Neovim, utilizing custom Tree-sitter injection queries for hybrid syntax highlighting 3 and Lua-based Telescope pickers for exposing the LSP’s semantic graph to the user.4

## **2\. The Architectural Landscape: Parser Selection and Constraints**

The foundation of any Language Server is its parser. The ability to accurately read, tokenize, and structure source code determines the fidelity of every feature downstream, from syntax highlighting to refactoring. For MyST, this challenge is compounded by its hybrid nature: it is a superset of Markdown that embeds rST-like semantic blocks.

### **2.1 The Rust Markdown Ecosystem: Stream vs. Tree**

The Rust ecosystem offers two primary candidates for Markdown parsing: pulldown-cmark and markdown-rs. A rigorous analysis of their internal architectures reveals a decisive divergence in their suitability for MyST.

#### **2.1.1 pulldown-cmark: The Stream-Based Standard**

pulldown-cmark stands as the incumbent standard within the Rust community, underpinning core utilities like rustdoc and mdBook.5 Its design philosophy prioritizes memory safety and execution speed through a zero-allocation pull parser architecture.7 Instead of constructing a complete AST in memory, pulldown-cmark emits a continuous stream of Events (e.g., Start(Tag::Paragraph), Text("content"), End(Tag::Paragraph)) as it iterates over the source text.8  
While this architecture yields exceptional performance benchmarks 9, it presents significant obstacles for parsing MyST. MyST relies heavily on **directives**—semantic blocks fenced by backticks or colons (e.g., {note}\`) that encapsulate content which may need to be parsed differently based on the directive type. For instance, a {code-block} directive treats its content as raw text to be highlighted, whereas a \`\`\`\`{admonition} directive treats its content as nested Markdown to be parsed recursively.  
Implementing this context-sensitive logic in a pull parser requires the consumer to maintain a complex, external state machine. The parser does not "know" it is inside a directive; the consumer must buffer events, detect the directive boundary, and then re-inject the content into a new parser instance or manipulate the event stream on the fly. This effectively forces the developer to write a secondary parser on top of pulldown-cmark, negating its primary advantage of simplicity. Furthermore, advanced MyST features like the eval-rst directive, which embeds reStructuredText, require a complete handover of control that fits poorly within a rigid event stream.10

#### **2.1.2 markdown-rs: The AST-Based Contender**

In contrast, markdown-rs adopts an architecture rooted in the generation of a concrete Abstract Syntax Tree (AST), adhering to the mdast (Markdown Abstract Syntax Tree) specification.1 This crate is a port of the JavaScript micromark and unified ecosystem, designed explicitly for extensibility and complex syntax transformations.  
The AST approach allows the parser to read the entire document structure into a traversable tree of nodes (e.g., Node::Root, Node::CodeBlock, Node::List). This is the critical enabler for MyST support. With an AST, identifying a directive becomes a matter of pattern matching on Node::CodeBlock nodes during a post-parsing traversal pass. If the "info string" of the code block matches a MyST directive pattern (e.g., {toctree}), the generic CodeBlock node can be transmuted into a specific MystDirective node without fighting the parser's internal state.11  
Research indicates that markdown-rs supports extensions such as GFM (GitHub Flavored Markdown), MDX, and Frontmatter out of the box.1 This suggests that the internal machinery for handling "syntax extensions" is already mature. While markdown-rs does not yet expose a public "plugin" API for dynamic loading of extensions 12, its modular design allows a Rust developer to import its internal lexer and parser components to construct a custom parser dialect. This "compile-time extension" capability is exactly what is required to bake MyST support directly into the LSP binary.

### **2.3 Verdict: The Necessity of markdown-rs**

The analysis concludes that **markdown-rs is the only viable foundation for a MyST LSP**. The requirements of MyST—nested parsing, semantic node transformation, and context-aware directive handling—map one-to-one with the capabilities of an AST-based architecture. Attempting to force pulldown-cmark to handle MyST would result in fragile, difficult-to-maintain state management code. Therefore, the roadmap proceeds with markdown-rs as the core parsing engine.

## **3\. Deep Dive: Extending markdown-oxide Architecture**

markdown-oxide is an existing Rust-based LSP designed for Personal Knowledge Management (PKM).13 It excels at managing "vaults" of loosely connected Markdown files using wikilinks. To support MyST, we must refactor its internal graph logic to support the rigorous, hierarchical structure of technical documentation.

### **3.1 The Graph Data Structure: From Mesh to Hierarchy**

markdown-oxide utilizes petgraph, a high-performance graph library, to model relationships between files.2 In its current PKM-centric implementation, the graph is likely a simple Directed Graph where nodes represent files and edges represent generic "links" (wikilinks).  
Technical documentation, however, is not a flat mesh. It is a structure composed of:

1. **The Build Tree (Toctree):** A strict hierarchy defined by {toctree} directives. This must be a Directed Acyclic Graph (DAG) rooted at index.md.  
2. **The Reference Mesh:** A web of cross-references defined by {ref} roles and (target)= anchors. This allows arbitrary cyclic connections (e.g., Chapter 1 referencing Chapter 5, and Chapter 5 referencing Chapter 1).  
3. **The Inclusion Graph:** A dependency graph defined by {include} directives. This also must be a DAG to prevent infinite recursion during rendering.

#### **3.1.1 Refactoring the petgraph Schema**

To accommodate these distinct relationship types, the markdown-oxide graph schema must be upgraded. We propose a MultiDiGraph (a directed graph allowing multiple edges between nodes) where edges carry typed metadata.  
**Proposed Rust Structs for Graph Architecture:**

Rust

use petgraph::graph::{NodeIndex, DiGraph};

// Extending the Node definition to hold MyST-specific state  
pub struct DocumentNode {  
    pub uri: String,  
    pub title: String,  
    pub is\_root: bool,       // Is this index.md?  
    pub has\_targets: Vec\<String\>, // List of anchors (target)= defined here  
}

// Defining semantic Edge types  
\#  
pub enum EdgeKind {  
    /// A soft link, e.g., \[text\](link) or {ref}\`label\`  
    Reference,  
    /// A structural parent-child link via {toctree}  
    Structure {  
        caption: Option\<String\>,  
        glob: Option\<String\>, // If the toctree uses globs  
    },  
    /// A content transclusion via {include}  
    Transclusion {  
        start\_line: Option\<usize\>,  
        end\_line: Option\<usize\>,  
    },  
}

// The Graph Type  
pub type MystWorkspaceGraph \= DiGraph\<DocumentNode, EdgeKind\>;

This schema allows the LSP to perform sophisticated queries that are impossible in a generic Markdown editor. For example, finding "Orphan Documents" becomes a graph query: identifying all nodes that have an in-degree of 0 for edges of type EdgeKind::Structure, excluding the root node.

### **3.2 Logic for Cycle Detection and Validation**

One of the primary value propositions of using Rust is the ability to perform expensive graph algorithms in real-time. In a Python-based system (like Sphinx), cycle detection often happens only at build time. In our Rust LSP, we can detect cycles as the user types.  
The Algorithm for include Cycles:  
When a user adds an {include} directive, the LSP must validate that this does not create a cycle.

1. **Trigger:** User modifies File A to include File B.  
2. **Graph Update:** Temporarily insert an edge A \-\> B of type Transclusion.  
3. **Validation:** Run petgraph::algo::is\_cyclic\_directed on the subgraph formed *only* by Transclusion edges.  
4. **Result:** If a cycle is detected, immediately publish a Diagnostic (Error) on the line containing the directive: "Circular inclusion detected: A \-\> B \-\>... \-\> A".  
5. **Rollback:** If cyclic, do not commit the edge to the permanent graph to maintain integrity.

### **3.3 Async Indexing with tokio**

markdown-oxide leverages tokio for asynchronous execution.15 This is critical for responsiveness. Parsing a large documentation set (e.g., 5,000 files) is CPU-intensive. If done on the main thread, the editor would freeze.  
The roadmap requires an **Actor-based architecture** for the indexing service:

* **The Main Loop:** Handles LSP requests (textDocument/completion, hover). It reads from a read-only view of the Graph.  
* **The Indexer Actor:** Runs on a background thread. It listens for textDocument/didChange and textDocument/didSave events.  
  * When a file changes, the Indexer re-parses *only* that file using markdown-rs.  
  * It diffs the new AST against the old AST to find changes in "Exports" (targets) and "Imports" (includes/toctrees).  
  * It acquires a write lock on the Graph, updates the nodes and edges, and releases the lock.  
* **Implication:** This separation ensures that "Go to Definition" (a read operation) never waits for a re-index (a write operation) to complete, guaranteeing the \<50ms latency requirement.

## **4\. Phase 1 Implementation: The myst-oxide-parser Crate**

Before modifying the LSP, we must build the parsing library. This library acts as the "middleware" between the raw text and the graph.

### **4.1 AST Transformation Logic**

Since markdown-rs creates a generic Markdown AST, we need a post-processing pass to "lift" generic nodes into MyST semantic nodes.  
Visitor Pattern Implementation:  
We implement a Visitor trait that traverses the markdown-rs AST.

Rust

impl MystVisitor for AstNode {  
    fn visit\_code\_block(\&mut self, block: \&CodeBlock) {  
        // Check for MyST directive syntax  
        // Regex: ^\\{(\[a-zA-Z0-9\_-\]+)\\}(.\*)$  
        if let Some(captures) \= MYST\_DIRECTIVE\_REGEX.captures(\&block.info) {  
            let directive\_name \= captures.get(1).unwrap().as\_str();  
              
            // Transform this node into a MystDirective  
            // This requires the AST enum to support a Custom variant  
            // or for us to map it to a separate semantic tree.  
            self.convert\_to\_directive(directive\_name, \&block.literal);  
        }  
    }  
}

### **4.2 Handling "Roles"**

MyST roles (e.g., {ref} or {doc}) are inline elements. Standard Markdown parsers often treat these as plain text or code spans depending on spacing.

* **Challenge:** The syntax {role}\`content\` is not standard CommonMark.  
* **Solution:** We must configure markdown-rs to enable the gfm and potentially write a custom Extension for markdown-rs that hooks into the tokenizer. If strictly extending the parser is too difficult due to private APIs, a pragmatic fallback is to parse the *text content* of generic Paragraph nodes using a lightweight regex pass to identify roles during the AST traversal. This "parser-within-a-parser" approach is often performant enough for inline elements.

## **5\. Phase 2 Implementation: LSP Capabilities**

With the graph and parser in place, we map MyST features to LSP endpoints.

### **5.1 workspace/symbol (The Search Engine)**

The standard workspace/symbol capability allows searching for classes and functions in code. For documentation, this maps to searching for **Targets** and **Headers**.

* **Current markdown-oxide:** Likely indexes headers (\#) and file names.  
* **MyST Extension:** We must index every (target)= label.  
* **Response Format:**  
  * Query: install  
  * Result 1: Installation (Header) \- File: install.md  
  * Result 2: install-requirements (Target) \- File: install.md (Line 50\)  
* **Optimization:** Use a Trie data structure for prefix lookups on target names. This ensures that searching a vault of 10,000 targets remains instant.

### **5.2 textDocument/definition (Go To Definition)**

This is the most highly requested feature for documentation authors.

* **Scenario 1: WikiLinks.** \[\[Note A\]\]. markdown-oxide handles this by looking up the filename.  
* **Scenario 2: MyST References.** {ref}target-label\`\`.  
  * **Logic:** Extract "target-label". Query the MystWorkspaceGraph for a node that contains this target in its has\_targets vector. Return the Location (File URI \+ Line Number).  
* **Scenario 3: Includes.** {include}../snippets/code.py.  
  * **Logic:** Resolve the relative path against the current file's URI. Check if the file exists on disk. Return the Location of that file. This turns the {include} directive into a clickable link, massively improving navigation in complex projects.

### **5.3 textDocument/completion (IntelliSense)**

Providing context-aware completion is where the static analysis shines.

* **Directive Completion:** When the user types \`\`\`{, trigger completion.  
  * **Source:** A static registry of built-in Sphinx directives (toctree, note, image, code-block) combined with any custom directives detected in the project configuration (if conf.py parsing is implemented).  
* **Reference Completion:** When the user types {ref} \`, trigger completion.  
  * **Source:** The global index of all (target)= anchors collected from the Graph.  
  * **UX:** Show the target label as the completion item, and the documentation (the text of the header it points to) as the description. This context helps users disambiguate between similar targets.

## **6\. Neovim Integration: The Treesitter Strategy**

While the LSP handles intelligence, Treesitter handles the visual understanding of the document structure. MyST's hybrid nature—embedding rST and other languages inside Markdown—presents a unique challenge for syntax highlighting.

### **6.1 The injections.scm Architecture**

Neovim's Tree-sitter integration allows for "Language Injections," where a specific node in the syntax tree is handed off to a different parser.3  
**The Problem:** Standard Markdown highlights fenced code blocks based on the info string (e.g., python). MyST uses the info string for the *directive name* (e.g., {note}), which is not a programming language.  
**The Solution:** We must write a custom injection query that maps MyST directives to their inner languages.  
**File:** \~/.config/nvim/queries/markdown/injections.scm

Scheme

; Extends the existing markdown injections

; Case 1: Semantic Directives (note, warning, admonition)  
; These contain nested Markdown. We inject 'markdown' into them.  
(fenced\_code\_block  
  (info\_string) @directive\_name  
  (code\_fence\_content) @injection.content  
  (\#match? @directive\_name "^\\\\{(note|warning|tip|attention|important)\\\\}$")  
  (\#set\! injection.language "markdown"))

; Case 2: The 'eval-rst' directive  
; This contains strict reStructuredText.  
(fenced\_code\_block  
  (info\_string) @directive\_name  
  (code\_fence\_content) @injection.content  
  (\#match? @directive\_name "^\\\\{eval-rst\\\\}$")  
  (\#set\! injection.language "rst"))

; Case 3: Code blocks with a specific language option  
; MyST allows: \`\`\`{code-block} python  
; We need to parse the directive to find the language argument.  
; This is HARD in regex.   
; Strategy: Fallback to highlighting the content as generic text   
; unless we write a custom grammar.

### **6.2 Developing tree-sitter-myst**

The injection queries above are a stopgap. They rely on regex matching of the info string, which is brittle. The robust long-term solution, as indicated by the prompt's request for optimization, is to develop a dedicated tree-sitter-myst grammar.  
This grammar would inherit from tree-sitter-markdown but add explicit rules for:

1. **Directives:** Parsing the {name} block, the :option: value block, and the body.  
   * *Benefit:* We can highlight keys (options) differently from values. We can syntax-highlight the arguments.  
2. **Roles:** Parsing {role}\`text\` as a distinct node type role, rather than inline\_code.  
   * *Benefit:* We can highlight the role name (ref) as a keyword and the content (target) as a variable.

**Implementation Strategy:**

* Fork tree-sitter-markdown (which is written in C).  
* Modify grammar.js to introduce directive and role rules.  
* Compile using tree-sitter-cli.  
* Register in Neovim via lua/vim/treesitter/language.lua.

This moves the complexity from runtime regex queries (slow, limited) to compile-time parser logic (fast, robust).

## **7\. Neovim Integration: Custom Telescope Workflows**

Telescope provides the UI for searching the data our Rust LSP has indexed. The default LSP pickers are generic. We need specialized tools for documentation workflow.

### **7.1 The "Reference" Picker**

We need a picker that allows a user to insert a reference to any section in the documentation.  
**Lua Implementation Logic:**

1. **Request:** The Lua client sends a custom command myst/getAllTargets (or repurposes workspace/symbol) to the LSP.  
2. **Response:** The LSP returns a list of targets with metadata (File, Header Text, Label).  
3. **Entry Maker:** A custom Lua function transforms this JSON response into a Telescope Entry.  
   * *Display:* Header Text (Label) \- File.md  
   * *Value:* The label string.  
4. **Action:** When the user presses \<CR\>, instead of opening the file, the picker inserts {ref}\`label\` at the current cursor position.

**Hypothetical Lua Code Snippet:**

Lua

local pickers \= require "telescope.pickers"  
local finders \= require "telescope.finders"  
local actions \= require "telescope.actions"  
local action\_state \= require "telescope.actions.state"  
local conf \= require("telescope.config").values

local function insert\_myst\_ref(opts)  
  opts \= opts or {}  
  pickers.new(opts, {  
    prompt\_title \= "Insert MyST Reference",  
    finder \= finders.new\_table {  
      results \= get\_lsp\_targets(), \-- Fetch from Rust LSP  
      entry\_maker \= function(entry)  
        return {  
          value \= entry.label,  
          display \= entry.text.. " (".. entry.label.. ")",  
          ordinal \= entry.text.. " ".. entry.label,  
        }  
      end,  
    },  
    sorter \= conf.generic\_sorter(opts),  
    attach\_mappings \= function(prompt\_bufnr, map)  
      actions.select\_default:replace(function()  
        actions.close(prompt\_bufnr)  
        local selection \= action\_state.get\_selected\_entry()  
        vim.api.nvim\_put({ "{ref}\`".. selection.value.. "\`" }, "c", true, true)  
      end)  
      return true  
    end,  
  }):find()  
end

This transforms Neovim from a text editor into a semantic authoring tool where links are created via search, not memorization.

## **8\. Evaluating Alternative Toolchains**

The prompt requires an evaluation of alternatives.

### **8.1 The Python Route (pylsp \+ myst-parser)**

One could simply run pylsp and try to hook myst-parser (the official Sphinx parser) into it.

* **Pros:** 100% spec compliance immediately. No need to rewrite parser logic in Rust.  
* **Cons:** **Performance Latency.** myst-parser is designed for batch builds (Sphinx). It creates a massive in-memory DOctree representation using docutils. Doing this on every keystroke to provide diagnostics is prohibitively expensive for large documentation sets (e.g., \>500 files). A user typing in Neovim expects feedback in \<100ms. A Python process parsing, analyzing, and pickling objects often takes seconds. Rust is the only viable path for real-time responsiveness in this domain.

### **8.2 The Tree-sitter Only Route**

Could we just use Tree-sitter queries for everything?

* **Pros:** Zero dependencies. No LSP binary to install.  
* **Cons:** **Lack of Global Context.** Tree-sitter is a *syntax* parser. It parses one file at a time. It has no concept of a "workspace." It cannot know that {ref}target-a\`\` in file\_b.md is valid because (target-a)= exists in file\_a.md. Only an LSP with a persistent Graph Index can solve cross-file references. Tree-sitter is necessary for highlighting, but insufficient for logic.

## **9\. Insights and Future Outlook**

### **9.1 The "Compiler-as-a-Service" Paradigm**

This roadmap highlights a trend where the Editor (Neovim) is taking over the responsibilities of the Build System (Sphinx). By implementing this LSP, we are essentially embedding a "micro-compiler" into the editor. This reduces the feedback loop from "Edit \-\> Save \-\> Switch Terminal \-\> Build \-\> Wait \-\> Check Browser" to "Edit \-\> See Red Squiggle". This immediacy fundamentally changes how documentation is written, encouraging more rigorous cross-referencing and structural integrity.

### **9.2 The "Headless" Documentation Graph**

By decoupling the documentation graph logic (petgraph) from the specific editor (Neovim), this Rust crate (myst-oxide) becomes a portable engine. It could eventually power a VS Code extension, a CLI linter, or even a WebAssembly-based browser editor. The investment in the Rust architecture pays dividends far beyond the immediate Neovim use case.

## **10\. Conclusion**

The roadmap to a high-performance MyST LSP for Neovim is clear but demanding. It requires rejecting the "easy" path of wrapping Python tools in favor of a "correct" path of building a Rust-native graph engine. By forking markdown-oxide, swapping pulldown-cmark for markdown-rs, and implementing a semantic graph over petgraph, we can build a tool that handles the complexity of MyST with the speed of Rust. When combined with custom Neovim Lua integration for Telescope and Tree-sitter, the result is a documentation environment that rivals dedicated IDEs in power while retaining the modal efficiency of Vim.

## **11\. Implementation Reference Tables**

### **Table 1: Comparative Analysis of Rust Parsers for MyST**

| Feature | pulldown-cmark | markdown-rs | Implication for MyST LSP |
| :---- | :---- | :---- | :---- |
| **Parsing Model** | Pull (Event Stream) | AST (Concrete Tree) | AST is required for directive nesting logic. |
| **Memory** | Minimal (Zero-copy) | Moderate (Tree alloc) | markdown-rs uses more RAM but enables manipulation. |
| **Extensibility** | Options Struct | Micromark/Unified | markdown-rs allows custom node types via forks. |
| **Ecosystem** | rustdoc, mdBook | deno, swc | markdown-rs is aligned with modern JS-based specs. |
| **Recommendation** | **REJECT** | **ADOPT** | pulldown creates unmanageable complexity for MyST. |

### **Table 2: MyST Construct Mapping to Technologies**

| MyST Feature | Rust Component (LSP) | Neovim Component (Editor) |
| :---- | :---- | :---- |
| **Directives** | MystNode::Directive struct | injections.scm (highlighting) |
| **Roles** | MystNode::Role struct | queries/markdown/highlights.scm |
| **References** | petgraph Edges (Graph) | Telescope Picker (insert\_ref) |
| **Toctree** | petgraph DAG (Validation) | Telescope Picker (toc\_navigation) |
| **Validation** | Diagnostic Publisher | Virtual Text / Signs (LSP Client) |

### **Table 3: Roadmap Phases and Deliverables**

| Phase | Goal | Key Tech | Deliverable |
| :---- | :---- | :---- | :---- |
| **1** | **Parsing** | markdown-rs | myst-oxide-parser crate that outputs MystAST. |
| **2** | **Graph** | petgraph, tokio | markdown-oxide fork with MultiDiGraph & Cycle Detection. |
| **3** | **LSP** | tower-lsp | workspace/symbol & goto/definition using the Graph. |
| **4** | **Editor** | lua, tree-sitter | telescope-myst.nvim & tree-sitter-myst grammar. |

#### **Works cited**

1. markdown \- crates.io: Rust Package Registry, accessed November 30, 2025, [https://crates.io/crates/markdown](https://crates.io/crates/markdown)  
2. petgraph \- crates.io: Rust Package Registry, accessed November 30, 2025, [https://crates.io/crates/petgraph](https://crates.io/crates/petgraph)  
3. Treesitter \- Neovim docs, accessed November 30, 2025, [https://neovim.io/doc/user/treesitter.html](https://neovim.io/doc/user/treesitter.html)  
4. Browse posts with telescope.nvim \- Jonas Hietala, accessed November 30, 2025, [https://www.jonashietala.se/blog/2024/05/08/browse\_posts\_with\_telescopenvim](https://www.jonashietala.se/blog/2024/05/08/browse_posts_with_telescopenvim)  
5. rust-unofficial/awesome-rust: A curated list of Rust code and resources. \- GitHub, accessed November 30, 2025, [https://github.com/rust-unofficial/awesome-rust](https://github.com/rust-unofficial/awesome-rust)  
6. markdown.rs \- source \- Rust Documentation, accessed November 30, 2025, [https://doc.rust-lang.org/stable/nightly-rustc/src/rustdoc/html/markdown.rs.html](https://doc.rust-lang.org/stable/nightly-rustc/src/rustdoc/html/markdown.rs.html)  
7. pulldown-cmark/pulldown-cmark: An efficient, reliable parser for CommonMark, a standard dialect of Markdown \- GitHub, accessed November 30, 2025, [https://github.com/pulldown-cmark/pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark)  
8. pulldown\_cmark \- Rust \- Docs.rs, accessed November 30, 2025, [https://docs.rs/pulldown-cmark/](https://docs.rs/pulldown-cmark/)  
9. Rust Markdown parser benchmark: comrak vs pulldown\_cmark \- Reddit, accessed November 30, 2025, [https://www.reddit.com/r/rust/comments/akwmln/rust\_markdown\_parser\_benchmark\_comrak\_vs\_pulldown/](https://www.reddit.com/r/rust/comments/akwmln/rust_markdown_parser_benchmark_comrak_vs_pulldown/)  
10. FAQ \- MyST Parser \- Read the Docs, accessed November 30, 2025, [https://myst-parser.readthedocs.io/en/latest/faq/index.html](https://myst-parser.readthedocs.io/en/latest/faq/index.html)  
11. markdown\_ast \- Rust \- Docs.rs, accessed November 30, 2025, [https://docs.rs/markdown-ast/latest/markdown\_ast/](https://docs.rs/markdown-ast/latest/markdown_ast/)  
12. Extending \`markdown-rs\` · wooorm markdown-rs · Discussion \#52 \- GitHub, accessed November 30, 2025, [https://github.com/wooorm/markdown-rs/discussions/52](https://github.com/wooorm/markdown-rs/discussions/52)  
13. Markdown Oxide: A Logseq inspired PKM system for your favorite text editor \- Reddit, accessed November 30, 2025, [https://www.reddit.com/r/logseq/comments/1cexbmq/markdown\_oxide\_a\_logseq\_inspired\_pkm\_system\_for/](https://www.reddit.com/r/logseq/comments/1cexbmq/markdown_oxide_a_logseq_inspired_pkm_system_for/)  
14. uhub/awesome-rust: A curated list of awesome Rust frameworks, libraries and software. \- GitHub, accessed November 30, 2025, [https://github.com/uhub/awesome-rust](https://github.com/uhub/awesome-rust)  
15. oxide \- crates.io: Rust Package Registry, accessed November 30, 2025, [https://crates.io/crates/oxide](https://crates.io/crates/oxide)