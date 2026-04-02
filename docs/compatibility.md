# rust-clorure compatibility

`rust-clorure` tracks `.cljc` compatibility as phased capability instead of a binary promise.

| Area | Phase 1 | Phase 2 | Phase 3 |
|------|---------|---------|---------|
| forms | yes | yes | yes |
| basic seq | yes | yes | yes |
| map/vector/set | yes | yes | yes |
| `defn`, `let`, `if`, `loop/recur` | yes | yes | yes |
| basic macros | yes | yes | yes |
| metadata | no | yes | yes |
| protocols | no | yes | yes |
| exception handling | no | yes | yes |
| multi-arity | no | yes | yes |
| multimethod | no | no | yes |
| advanced macro helpers | no | no | yes |
| AOT export support | no | no | yes |

Host conditionals are handled with `:clorure` as the first-class branch. Reader conditionals continue to accept fallback branches for compatibility-oriented sources, while host interop remains explicitly opt-in through registered builtins and adapters.
