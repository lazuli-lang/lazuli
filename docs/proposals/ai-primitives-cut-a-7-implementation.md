# Cut A.7 Implementation Plan — `expose http` on agent

**Status**: Draft. Sequencing the design from
`docs/proposals/ai-primitives-cut-a-7.md` (architect-approved with
notes applied) into phased implementation work. Submit for approval
before any code lands.

**Depends on**: Cut A implementation
(`docs/proposals/ai-primitives-v0-implementation.md`) Phases 1–3.
Cut A.7 cannot start until Cut A has at minimum an `Agent` AST/IR
shape.

**Scope**: Cut A.7 only — `expose http` child of `agent`, with
HTTP method, path, route slots, optional audience, optional rate
limit override. Plus the migration of existing `api.method: String`
to a typed `HttpMethod` enum.

## 0. Discovery

The Cut A foundation that A.7 builds on:

| Layer | Cut A status | What A.7 adds |
|---|---|---|
| Lexer / parser | Cut A Phase 1 ships `agent`-block hand-written line-walker | New child `expose http` recognizer inside the same line-walker |
| AST | Cut A Phase 1 ships `Agent` node | Add `expose: Option<AgentExpose>` field on `Agent` |
| IR | Cut A Phase 2 ships `Agent` IR | Add `expose_http: Option<HttpExposure>` field |
| Doctor | Cut A Phase 3 ships agent-tool diagnostics | Add five A.7 diagnostics; reuse policy-lattice + path-conflict helpers |
| LSP | Cut A Phase 4 adds file-local agent diagnostics | Add A.7 file-local checks (slot binding, method/streaming) |
| Inspect | Cut A Phase 5 ships `--expand=summary` for agents | Add `expose_http` to summary, new optional `--expand=expose` |
| Fixture | Cut A Phase 6 updates fixture with tools/evals | Remove `api customer_summary_stream`, add `expose http` to `summarize_customer` |
| Codegen | Cut A Phase 7 stubs agent codegen | Carry `expose_http` through to runtime IR consumption |

The migration of `api.method: String → HttpMethod` enum is a
**separate concern bundled into A.7's IR delta** because A.7 needs
the typed enum and there's no value in keeping `api`'s field
loosely typed once the enum exists.

## 1. Implementation strategies

Cut A.7 has only one viable strategy: extend the foundation Cut A
created. There is no text-pattern fallback because the diagnostics
A.7 introduces (cross-feature path conflict, slot binding) require
typed IR.

Don't try to land A.7 before Cut A's Phase 2 (IR) is in.

## 2. Phase sequencing

```
  Phase 1: AST + parser extension
     ↓ (deliverable: `expose http` block parses on agent)
  Phase 2: IR + lowering + HttpMethod enum migration
     ↓ (deliverable: agent's expose_http appears in inspect JSON)
  Phase 3: Doctor diagnostics
     ↓ (deliverable: path conflicts and slot bindings checked)
  Phase 4: LSP file-local diagnostics
     ↓ (deliverable: local path conflicts caught while typing)
  Phase 5: Inspect projections
     ↓ (deliverable: --expand=summary, --expand=expose work)
  Phase 6: Fixture migration
     ↓ (deliverable: full-capsule uses expose http; api block removed)
  Phase 7: Runtime + codegen hand-off
     ↓ (deliverable: runtime can mount the endpoint from IR)
```

Phases 1–3 are sequential (~6 days). Phases 4–7 fan out from Phase
3 and total ~3 days.

## 3. Phase 1 — AST + parser extension

**Goal**: indented `expose http` block parses to an `AgentExpose`
AST node attached to `Agent`.

### 3.1 Files touched

- `crates/lazuli_syntax/src/ast.rs` — extend the `Agent` struct
  (Cut A introduced) with:

  ```rust
  pub struct Agent {
      // Cut A fields...
      pub expose: Option<AgentExpose>,
  }

  pub struct AgentExpose {
      pub method: HttpMethod,
      pub path: String,
      pub route_slots: Vec<TypedSlot>,
      pub audience: Option<String>,
      pub rate_limit_override: Option<String>,
      pub span: Option<Span>,
  }

  pub enum HttpMethod {
      Get,
      Post,
      Put,
      Patch,
      Delete,
  }
  ```

  `HttpMethod` is **new** in AST as well as IR (today
  `Api.method: String`). The AST migration is part of this phase.

- `crates/lazuli_syntax/src/parser.rs` — inside the Cut A
  `parse_agent` line-walker, add a child-recognizer that matches
  `expose http` (indent +2, exactly that header), opens a nested
  indented block (+4), and dispatches to:

  ```rust
  fn parse_agent_expose(
      lines: &[SourceLine],
      start: usize,
  ) -> Result<(AgentExpose, usize), ParseError>;
  ```

  Children parsed inside: `method <METHOD>`, `path "<string>"`,
  `route <slot>: <Type>`, `audience <ident>`, `rate_limit "<str>"`.

- Same file — migrate `parse_api`'s `method` field from
  `String` to `HttpMethod` parsing (string → enum). The Api AST
  shape changes; downstream consumers (analyzer, codegen) update
  in Phase 2.

### 3.2 Snapshot tests

In `crates/lazuli_syntax/src/parser.rs` test module:

- `agent_with_expose_http_minimal_parses` — just `method` and
  `path`.
- `agent_with_expose_http_full_parses` — all children.
- `agent_with_expose_http_audience_parses`.
- `agent_rejects_unknown_method_in_expose` — `method FROBNICATE`.
- `agent_rejects_expose_http_without_method` — required child.
- `agent_rejects_expose_http_without_path` — required child.
- `api_method_string_migrated_to_enum` — existing fixture parses.

### 3.3 Backward compatibility

The `api` block today accepts any string for `method`. Real
fixtures use `GET | POST | PUT | PATCH | DELETE`. The migration
rejects unknown method strings — this is intentional. Verify
against:

```bash
grep -rn 'method ' examples/ | grep api
```

If any fixture uses a non-canonical method string, fix the fixture
or document it as a closed-catalog enforcement decision.

## 4. Phase 2 — IR + lowering + HttpMethod migration

**Goal**: `Agent.expose_http: Option<HttpExposure>` lowers from AST
to IR. `Api.method` migrates from `String` to `HttpMethod`. Inspect
reports both.

### 4.1 Files touched

- `crates/lazuli_ir/src/lib.rs`:
  - Add `HttpMethod` enum.
  - Add `HttpExposure` struct.
  - Add `Agent.expose_http: Option<HttpExposure>` field.
  - Migrate the existing `api` IR shape (`crates/lazuli_ir/src/lib.rs`
    line 1610 area) from `method: String` to `method: HttpMethod`.

  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
  #[serde(rename_all = "UPPERCASE")]
  pub enum HttpMethod {
      Get,
      Post,
      Put,
      Patch,
      Delete,
  }

  pub struct HttpExposure {
      pub method: HttpMethod,
      pub path: String,
      #[serde(default, skip_serializing_if = "Vec::is_empty")]
      pub route_slots: Vec<TypedSlot>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub audience: Option<String>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub rate_limit_override: Option<String>,
      #[serde(default, skip_serializing_if = "Option::is_none")]
      pub span_ref: Option<SpanRef>,
  }
  ```

- `crates/lazuli_analyzer/src/lib.rs`:
  - Extend `lower_agent` (Cut A introduced) to lower `expose` if
    present.
  - Update `lower_api` (existing) to consume the new `HttpMethod`
    enum.

### 4.2 IR delta classification

- `LZIR_SCHEMA`: this is the load-bearing decision. Two routes:

  - **Minor bump with compatibility shim**: keep `method: String`
    serializable for backward compatibility, but lower from string
    to enum on read; deserialize with `#[serde(try_from = "String")]`.
    Adds shim code; preserves old IR JSONs.
  - **Major bump**: cleanly migrate. Old serialized IRs become
    incompatible. Per `docs/ir-abi.md:64`, major bumps require
    re-lowering from `.lzi` source; not from IR.

  **Recommendation**: minor bump with shim. The migration only
  rejects strings outside the closed set; existing fixtures use
  uppercase canonical strings already (`GET | POST | ...`), so the
  shim is a deserialize-only convenience and adds ~20 lines.

  **Wire-format note (cite in `docs/ir-abi.md` minor-bump entry)**:
  the shim deserializes legacy `String` form into the enum
  (`#[serde(try_from = "String")]`); serialization emits uppercase
  string (`#[serde(rename_all = "UPPERCASE")]`). The on-wire form
  before and after this cut is bytewise-identical for the canonical
  five methods. Old IR JSON consumers continue to read newer IR;
  newer consumers reject legacy invalid strings. This is the
  intentional incompatibility surface and the reason this is a
  minor bump and not a no-op.

- `LZI_LANG`: minor bump (additive `expose http` block).

### 4.3 Snapshot tests

- `lower_agent_with_expose_http_minimal_lowers`.
- `lower_agent_with_expose_http_full_lowers`.
- `ir_serializes_http_method_as_uppercase`.
- `ir_deserializes_legacy_string_method_on_api`.
- `inspect_summary_includes_expose_http`.

### 4.4 Open questions for Phase 2

- **Q-impl-7-1**: Should `audience` be a `String` or a typed
  reference? Today `.lzx` audiences are bare identifiers
  (`admin`, `public`, etc.) without a closed catalog. For now,
  keep `String`; revisit if the audience catalog becomes formal.

## 5. Phase 3 — Doctor diagnostics

**Goal**: `lazuli doctor` emits the four cross-feature A.7
diagnostics.

### 5.1 Files touched

- `crates/lazuli_cli/src/doctor.rs`:
  - Add `agent_expose_diagnostics(facts: &PackageFacts) ->
    Vec<DoctorDiagnostic>`.
  - Inside, fan out to:
    - `agent_expose_path_conflict_cross_feature_diagnostics_for(...)`.
    - `agent_expose_audience_unknown_diagnostics_for(...)`.

  Reuse the existing route-collision helper (the LZX route facts
  already collect path normalization for `app.lzi auth_failed_redirect`
  and `not_found` redirects — extract the helper if it's local).

### 5.2 Diagnostics

| Id | Pipeline | Implementation |
|---|---|---|
| `agent_expose_path_conflict_cross_feature_diagnostics` | doctor | Walk all `.lzi` agents with `expose_http` plus all `api` blocks plus all `.lzx route` declarations. Normalize paths (strip placeholder names, retain type signature). Reject collisions across feature boundaries. |
| `agent_expose_audience_unknown_diagnostics` | doctor | Collect audiences from all `.lzx` surfaces and routes. Reject `expose http audience X` if `X` is not in the set. |

### 5.3 Doctor test pattern

In `crates/lazuli_cli/src/doctor.rs` `mod tests`:

- `doctor_rejects_expose_http_path_colliding_with_api`.
- `doctor_rejects_expose_http_path_colliding_with_other_agent`.
- `doctor_normalizes_placeholder_name_when_checking_collisions`.
- `doctor_accepts_same_path_different_method_on_collision_check`
  (two endpoints same path, different methods, allowed).
- `doctor_rejects_unknown_audience_on_expose_http`.
- `doctor_accepts_known_audience_from_surface`.

### 5.4 Open questions for Phase 3

- **Q-impl-7-2**: When two paths collide (`/api/customers/:id` and
  `/api/customers/:customer_id`, both `Customer.ID`), reject or
  warn? Architect's grade implied reject. Decision: reject. The
  paths are functionally identical from the gateway's perspective.

## 6. Phase 4 — LSP file-local diagnostics

**Goal**: file-local A.7 diagnostics fire while typing.

### 6.1 Files touched

- `crates/lazuli_lsp/src/lib.rs`:
  - Add `agent_expose_diagnostics(source: &str) -> Vec<Diagnostic>`.
  - Inside, fan out to file-local checks:
    - `agent_expose_path_conflict_local_diagnostics`.
    - `agent_expose_slot_unbound_diagnostics`.
    - `agent_expose_slot_must_use_route_diagnostics`.
    - `agent_expose_method_streaming_mismatch_warning`.
  - Wire into the existing diagnostics dispatcher (the function
    around line 333 in lib.rs).

### 6.2 Diagnostics

| Id | Implementation |
|---|---|
| `agent_expose_path_conflict_local_diagnostics` | Walk the file. Collect all agent `expose_http.path` and all `api.path`. Reject duplicates (same string, same normalized form). |
| `agent_expose_slot_unbound_diagnostics` | For each `:slot` placeholder in `path`, require a matching `route slot:` declaration inside the `expose http` block. |
| `agent_expose_slot_must_use_route_diagnostics` | If a path slot's name matches an `input` slot but no `route` slot, reject with the migration suggestion (use `route` for path bindings, not `input`). |
| `agent_expose_method_streaming_mismatch_warning` | When `method GET` and the agent has `output stream`, warn. |

### 6.3 LSP test pattern

In existing test module:

- `agent_expose_local_path_conflict_caught`.
- `agent_expose_slot_unbound_caught`.
- `agent_expose_slot_must_use_route_caught_with_input_slot_collision`.
- `agent_expose_method_get_streaming_warns`.

## 7. Phase 5 — Inspect projections

**Goal**: `--expand=summary` reports `expose_http`. New
`--expand=expose` projection lists the unified HTTP route table.

### 7.1 Files touched

- `crates/lazuli_cli/src/main.rs`:
  - Extend `AgentSummary` (Cut A introduced) with `expose_http:
    Option<ExposeSummary>`.
  - Add `--expand=expose` handler that walks all features, collects
    every `api.path + method + auth` and every `agent.expose_http.
    path + method + auth`, and emits a unified table.

### 7.2 Snapshot tests

JSON snapshot tests:

- `inspect_summary_includes_agent_expose_http_when_present`.
- `inspect_expose_lists_apis_and_exposed_agents_unified`.
- `inspect_expose_omits_agents_without_expose_http`.

### 7.3 Single-pass guarantee

Same rule as Cut A's plan: `inspect --expand=summary` stays
single-pass. `--expand=expose` may walk multiple features but
should not require workspace IR.

## 8. Phase 6 — Fixture migration

**Goal**: `examples/full-capsule/full-capsule.lzi` uses `expose
http` on the agent; the duplicate `api customer_summary_stream`
block is removed.

### 8.1 Files touched

- `examples/full-capsule/full-capsule.lzi`:
  - Remove the `api customer_summary_stream` block. Verify line
    numbers at implementation time with
    `grep -n "api customer_summary_stream" examples/full-capsule/full-capsule.lzi`
    — line numbers drift between proposal and implementation as the
    fixture grows.
  - Extend `agent summarize_customer` (similarly find with
    `grep -n "agent summarize_customer"`) with:

    ```lazuli
    expose http
      method POST
      path "/api/customers/:customer_id/summary"
    ```

- `examples/full-capsule/full-capsule.lzi` callers — none today
  reference the api block.
- `examples/full-capsule/api/stream_customer_summary.go` — remove
  the handler file. The runtime team's parallel work materializes
  the dispatch.
- `tools/generate-fixtures.ps1` — confirm it doesn't strip
  `expose http` shapes.

### 8.2 Verification

- `cargo run -q -p lazuli_cli -- check examples/full-capsule/full-capsule.lzi`
  passes.
- `cargo run -q -p lazuli_cli -- doctor examples/full-capsule`
  emits zero errors and no unexpected warnings.
- `cargo run -q -p lazuli_cli -- inspect examples/full-capsule/full-capsule.lzi --expand=expose --format=json | jq` returns the unified route table including the agent.

## 9. Phase 7 — Runtime + codegen hand-off

**Goal**: the runtime team consumes the new IR shape and mounts
the endpoint.

### 9.1 Files touched

- `crates/lazuli_codegen_go/src/lib.rs` — if the agent-rendering
  path exists (post-Cut A), extend it to also generate an HTTP
  mount when `expose_http` is present. If not present yet, add a
  TODO entry referencing **both** this plan
  (`docs/proposals/ai-primitives-cut-a-7-implementation.md`) and
  Cut A's plan
  (`docs/proposals/ai-primitives-v0-implementation.md`) so the
  runtime team has a clear handoff anchor for the agent
  surface as a whole.

- Coordinate with the runtime team:
  - The runtime needs to know the agent's `expose_http.method`,
    `path`, `route_slots`, and the agent's resolved
    `policy`/`rate_limit`/`output` to mount the endpoint.
  - All of the above are in IR after Phase 2; the runtime
    consumes IR JSON.

### 9.2 No-op cases

- Agents without `expose_http` produce no runtime change.
- The existing `api` block (any `api`, not just the deleted one)
  continues to mount as before.

## 10. Estimates

| Phase | Effort | Blockers |
|---|---|---|
| 1 — AST + parser extension | 2 days | Cut A Phase 1 |
| 2 — IR + lowering + HttpMethod migration | 2 days | Cut A Phase 2; LZIR_SCHEMA bump decision |
| 3 — Doctor diagnostics | 2 days | Cut A Phase 3; route-conflict helper extraction |
| 4 — LSP file-local | 1.5 days | none |
| 5 — Inspect projections | 1 day | Phase 2 |
| 6 — Fixture migration | 0.5 day | Phase 2 |
| 7 — Runtime hand-off | 0.5 day | none (TODO is acceptable) |
| **Total** | **9.5 days for one engineer** | Cut A landed |

With Cut A done, A.7 is small. Phases 1–3 sequential (~6 days);
4–7 fan out (~3 days). Two engineers: ~6 days end-to-end.

## 11. Acceptance criteria

- Cut A Phases 1–3 have shipped.
- `expose http` block parses, lowers, and is honored.
- All four doctor diagnostics implemented and tested.
- All four LSP diagnostics implemented and tested.
- `lazuli inspect --expand=expose` works and reports correctly.
- `examples/full-capsule/full-capsule.lzi` uses `expose http`;
  the `api customer_summary_stream` block is gone.
- `cargo fmt --check`, `cargo test -q`, and
  `cargo run -q -p lazuli_cli -- doctor examples/full-capsule`
  all pass.
- `docs/grammar.lzi.md §14 (Agent)` adds `expose http` child.
- `docs/canonical-semantics.md §Working With Agents` documents
  the `expose http` shortcut.
- `docs/invariants.md` agent invariant adds `expose http` as
  optional.
- `docs/design-decisions.md` records: *agent `expose http` is the
  shortcut for trivial agent-dispatch APIs; non-trivial APIs that
  call agents stay as `api` blocks. The boundary is "does the
  handler do work beyond translating HTTP to agent dispatch?"*

## 12. Open questions consolidated

- **Q-impl-7-1**: `audience: String` vs typed reference. Decision:
  String for now.
- **Q-impl-7-2**: Cross-feature path collision with placeholder
  type-equivalence: reject. Decided.
- **Q-impl-7-3**: `LZIR_SCHEMA` bump shape — minor with shim. The
  shim is ~20 lines; acceptable.

## 13. Risks

- **Runtime mounting depends on parallel work**. Mitigation:
  language-side cut is self-sufficient (IR exposes the contract,
  inspect projects it). Runtime team consumes IR at its own pace.
- **`HttpMethod` migration breaks existing IR golden snapshots**.
  Mitigation: regenerate goldens in the same cut. Any external
  consumer of IR JSON sees uppercase string in the new schema;
  the legacy lowercase-string form (if any) is handled by the
  serde shim.
- **Fixture removal of the api handler file disrupts users
  copying the fixture**. Mitigation: document in the commit
  message that the handler file moved into the runtime;
  generated apps shouldn't have the file at all.

## 14. Non-goals

- `expose grpc`, `expose websocket`, `expose graphql`. Per
  proposal §Non-goals.
- `expose http` on `command`, `query`, `webhook`. Per proposal.
- Auto-deprecation of trivial-agent-handler `api` blocks. A
  `--strict-apis` flag is reserved for a future cut.
- OpenAPI schema generation. Adapter / publication concern.

## 15. Sequencing summary

```
Cut A Phase 1-3 land
   ↓
Approve plan
   ↓
Phase 1 (2d) — AST + parser
   ↓
Phase 2 (2d) — IR + HttpMethod migration
   ↓
Phase 3 (2d) — doctor
   ↓
[Phases 4, 5, 6, 7 in parallel — 1.5 + 1 + 0.5 + 0.5 = 3.5d]
   ↓
Acceptance gate (re-grade by lazuli-language-architect)
   ↓
Cut A.7 ships
```
