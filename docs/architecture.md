# rust-clorure architecture

`rust-clorure` follows the same high-level pipeline split as `cljs`.

1. `clorure.reader`
   Reads source into pure forms with location metadata.
2. `clorure.analyzer`
   Accepts forms and a compiler environment, then produces AST nodes.
3. `clorure.compiler`
   Dispatches AST into a backend target.
4. `clorure.backend`
   Hosts bytecode and Rust backends behind a shared compiler boundary.
5. `clorure.runtime`
   Stores `Value`, `Var`, and `Namespace` as runtime-level entities instead of exposing Rust symbols directly.
6. `clorure.repl`
   Connects reading, analysis, and compilation into a host-facing API.

The initial codebase keeps analyzer and backend intentionally small while the reader and form model are fleshed out first. This matches the dependency chain in the specs: reader output becomes the stable contract for later analyzer and runtime work.
