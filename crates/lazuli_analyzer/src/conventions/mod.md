`conventions [...]` auto-synthesis cluster.

The closed-catalog convention vocabulary (`crud`, `me`) lifts a
resource's field list into the canonical Command / Query IR shapes
described by `ir-resource-conventions-crud.md` (5 entries) and
`ir-resource-conventions-me.md` (1 entry). The synthesis pass is
purely additive — author-written same-named items always win
(override semantics §6).

## RULE-VOCAB-03 zero-workflow guarantee

Every `if` / `match` in this module is **authoring-time dispatch** —
it picks which IR node shape the synth pass emits. The emitted IR
itself contains zero control flow; downstream codegen lowers each
synthesized command/query to one fixed SQL per crud §7 / me §7.

## Module layout

This module is the single public entry — `synthesize_conventions` —
plus the per-cluster helpers in the sibling modules:

* `diagnostics` — `ConventionSynthDiagnostic` enum + `CrudSynthDiagnostic` alias.
* `me_mode` — `MeMode`, `classify_me_mode`, `build_lookup_my_query`.
* `fields` — `CategorisedFields`, `categorize_fields`, input projection.
* `owner_scope` — `OwnerScopeResolution`, `resolve_owner_scope`, test exports.
* `signature` — `CanonicalReturn` + `check_*_signature_mismatch`.
* `crud` — `build_*_command` / `build_*_query` + shared synth defaults.
