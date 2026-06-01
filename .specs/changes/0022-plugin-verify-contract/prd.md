---
id: 0022
title: Plugin verify contract — end-to-end wiring proof + compile-time adapter contract check
type: prd
stage: 4 of 5 (Plugin Platform)
status: ready
created: 2026-06-01
---

# PRD — Plugin verify contract

## Problem
A plugin can be fully declared and still be **silently inert**. 0019 made resolution one mandatory pipeline with loud failures; 0020 made doctor and codegen share ONE authoritative resolver; 0021 made the manifest a typed multi-kind shape (`implements` / `[binds]` / `[env]` are now parsed fields, not prose). What is STILL missing is anything that asserts the *whole chain holds end-to-end*: manifest-found → typed-parsed → alias-resolved → codegen-arm-reached → runtime-import-emitted. Each link is verified in isolation by some test somewhere, but no single command walks a real project's `[plugins]` and reports, per plugin, that every link holds.

The most acute gap is **adapter binding**. An adapter plugin — mercadopago, smtp, object-store, google-maps, sms-twilio, the two social providers — declares the capability it fulfils as `implements = ["payments.PaymentGateway"]` and a `[binds]` interface. Today (even after 0021 makes those typed fields) **the compiler never checks them against anything**. Adapter binding is a pure runtime string lookup: the Go `init()` calls `lazuli.RegisterAdapter("@lazuli/plugin-mercadopago", ...)` and the facade does `LookupAdapter(ref)` at request time. There is NO compile-time interface check (`crates/lazuli_cli/src/lazurite_codegen.rs` packs the `go_module` for a side-effect import; `crates/lazuli_codegen_go/src/emitter/root/main_go.rs:273-308` emits `_ "<go_module>"` — and that is the entire static story). So an adapter whose `implements` names a bucket interface that does not exist, or whose declared capability has no corresponding registry binding in the app, compiles clean and fails as `ErrAdapterMissing` at the first live request — exactly the "declared but silently inert" class 0019 set out to kill, surviving in the adapter dimension.

Concretely reproducible against hostpoint: it declares 8 active plugins in `Lazurite.toml [plugins]` (mercadopago, google-maps, scalars-br, object-store, smtp, sms-twilio, social-google, social-apple). Their manifests are *heterogeneous prose today* — mercadopago/object-store/google-maps use a top-level `implements` scalar, smtp uses `[binds].interface`, social-google uses `[provides].go_interface`, sms-twilio uses `[provides]` with no interface at all. 0021 normalises these into typed `implements`/`[binds]`/`[env]`. Nothing reads them to confirm the wiring graph is whole. A developer who fat-fingers `implements = ["payments.PaymentGatway"]` gets zero feedback until production.

## Why now (or why ever)
This is the **prove/ship** capstone of the Plugin Platform track. 0019-0021 built the trustworthy machinery (one pipeline, one resolver, one typed manifest). 0022 is the command that *proves the machinery is actually wired for a given project* and the doctor check that makes the adapter-contract dimension fail at compile time instead of at the first paid checkout. Without it, every guarantee 0019-0021 bought stays a property of the framework's own tests, never surfaced to the pilot author building on top. hostpoint cannot answer "are my 8 plugins actually wired, or just declared?" — and that question is the difference between a deploy that serves payments and one that 500s on the first Pix.

It is also a CLASS fix: `lazuli plugin verify` is the single place an author (human or agent) checks plugin wiring, and `PLUGIN-CONTRACT-001` is the single diagnostic that turns a misdeclared adapter into a build-time error for every plugin kind and every author, forever.

## Outcome — done means
1. **`lazuli plugin verify [--plugin <ns>]`** exists. It exercises the REAL authoritative resolver (0019's single pipeline / 0020's shared resolver) over a project's `[plugins]` and reports, per plugin, a PASS/FAIL across every wiring link: (a) manifest found + parsed (typed per 0021), (b) semantic aliases (if any) resolve, (c) adapter `implements`/`[binds]` declared and well-formed, (d) the `go_module` is resolvable and its side-effect import WILL emit, (e) required `[env]` vars are present in the app's env contract. On any FAIL the command exits non-zero and names the **exact broken link**. Both `--json` and human output.
2. **Compile-time adapter-contract check** (`PLUGIN-CONTRACT-001`, doctor, error severity). When a plugin declares `implements = ["payments.PaymentGateway"]` / `[binds]`, the check verifies — as far as is statically possible WITHOUT compiling Go — that (i) the declared interface name matches a KNOWN bucket interface (`payments.PaymentGateway`, `storage.ObjectStore`, `maps.Geocoder`, `notifications.EmailSender`, …), and (ii) the app's registry binding for that capability points at this plugin. The diagnostic fires when an adapter is declared but its `implements` interface is unknown, or its capability binding is missing. Registered in `lazuli_keywords` facets (bridge test green), carries a `//!` trigger-cue header (module_headers test green), and is wired into the run aggregator.
3. **Honest static limit, stated.** The Rust compiler cannot run `go build`, so `verify` proves the DECLARED contract + the wiring graph — not that the Go `Adapter` type actually satisfies the interface. The runtime-side `var _ payments.PaymentGateway = (*Adapter)(nil)` assertion in the plugin's `adapter.go` is the Go-side complement that closes the method-set gap. `verify` says so in its own output and the docs say so too.
4. **TEACH:** `docs/plugin-authoring.md` documents `lazuli plugin verify` (what each link means, how to read a FAIL) and the `PLUGIN-CONTRACT-001` contract check (the two-part static+runtime story).
5. **ENFORCE:** tests prove `verify` PASSES on hostpoint's resolvable plugins and FAILS — with the right broken-link message — on a deliberately-misdeclared adapter fixture.

## Non-goals
- **Running `go build` / actual Go method-set verification.** Out of scope and impossible from Rust; the runtime compile-time assertion (`var _ Interface = (*Adapter)(nil)`) is the complement, not this spec's job to invoke.
- **Defining the typed manifest schema.** That is 0021. 0022 CONSUMES 0021's typed `implements`/`[binds]`/`[env]` fields; it does not define them.
- **Building the single authoritative resolver.** That is 0020. 0022 CALLS it; it does not build it.
- **Scaffolding new plugins (`lazuli plugin new`).** That is 0023.
- **Widening the bucket-interface catalog** or adding new capability slots — the check validates against the buckets that exist in `runtime/go/lazuli/{payments,storage,maps,notifications,auth}/`; growing that catalog is a runtime concern.
- **TS-side adapter verification.** v1 verifies the Go wiring graph (side-effect imports + Go bucket interfaces). TS adapter parity is a follow-up.

## User stories
- As a hostpoint dev, I run `lazuli plugin verify` and see all 8 plugins reported PASS/FAIL with their real wiring status — so I know before deploy that mercadopago's payment gateway is actually reachable, not just declared.
- As a plugin author, when I write `implements = ["payments.PaymentGatway"]` (typo) or point `[binds].interface` at an interface no bucket exports, `lazuli check` fails with `PLUGIN-CONTRACT-001` naming the unknown interface — at build time, not at the first request.
- As an agent generating a pilot, `lazuli plugin verify --json` gives me a machine-readable per-plugin per-link map so I can self-correct a misdeclared adapter without a human in the loop.
- As a framework maintainer, the misdeclared-adapter fixture test guarantees the contract check can never silently regress to passing a broken binding.

## Constraints
- **No false negatives on the silent-but-legitimate cases.** A semantic-only plugin (scalars-br) has no `implements`/`[binds]` — `verify` reports its semantic links and does NOT invent an adapter FAIL. A plugin declared in anticipation but not yet bound is a documented `verify` state, consistent with `PLUGIN-UNUSED-001`'s warning-not-error stance.
- **Reuse the authoritative resolver (0020) and the typed manifest loader (0021).** `verify` must NOT re-implement resolution or re-parse the manifest with a private path; it walks the same code real codegen walks, or it proves nothing.
- **`PLUGIN-CONTRACT-001` is error severity** (a misdeclared adapter is a wiring bug, not a style nit) and follows the existing `PLUGIN-*` diagnostic conventions (anchored at `Lazurite.toml` or the plugin manifest, message names the plugin + the fix).
- **Honest about the static boundary** everywhere it surfaces: `verify` output, the doctor message, and the docs all state that method-set conformance is the runtime assertion's job.

## Open questions
None. The verify-link set, the contract-check shape, and the static/runtime split are decided in the ADR.
