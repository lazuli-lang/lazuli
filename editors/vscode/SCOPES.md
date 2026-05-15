# Lazuli — TextMate Scope Reference

Generated reference for theme authors and developers extending the
Lazuli VS Code grammar. Lists every scope name the grammar emits +
its semantic role + the contexts where it appears.

The two grammars covered here:

- `syntaxes/lazuli.tmLanguage.json` — main grammar for `.lzi` and `.lzx`
  files (`source.lazuli`). Emits ~133 distinct scope names.
- `syntaxes/lazurite-manifest.tmLanguage.json` — TOML overlay for
  `Lazurite.toml` (`source.lazurite-toml`). Emits 8 distinct scope
  names (4 `*.lazuli`/`*.toml` overlay scopes + 4 standard TOML
  punctuation scopes).

## Quick reference (theme author summary)

If you're authoring a theme and want Lazuli files to look right, color
these scope families and you'll cover 95% of tokens:

| Scope family                                                         | Suggested role / color                                                          |
| -------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| `keyword.control.declaration.structural.lazuli`                      | Top-level kind (feature/app/etc.) — use your "class declaration" color          |
| `keyword.control.section.lazuli`                                     | Block-opener (params/filters/audit/etc.) — use your "keyword.control" color     |
| `keyword.control.statement.lazuli`                                   | Statement keywords (creates/updates/order/etc.) — use your "keyword" color      |
| `entity.name.tag.decorator.lazuli`                                   | Decorator namespaces (@policy/@scope/@fn/etc.) — use your "tag/decorator" color |
| `entity.name.function.named-block.lazuli`                            | Named-block name (the `X` in `command X`) — use your "function name" color      |
| `entity.name.function.statement.*.lazuli` family                     | Sub-block statement keywords (cookie/limits/policy/etc.) — "keyword" color      |
| `entity.name.type.X.lazuli` family                                   | Top-level entity names (feature/app/aggregate names) — "type name" color        |
| `entity.name.label.X.lazuli` family                                  | Identifier labels (rbac/audience/integration/slot) — "tag/label" color          |
| `entity.name.reference.X.lazuli` family                              | Cross-references (`@runtime/...`, model paths, anchors) — "namespace" color     |
| `support.type.primitive.lazuli`                                      | Built-in scalar types (ID/Text/Integer/etc.) — "type" color                     |
| `support.type.ui.lazuli`                                             | UI component types (Form/Table/SidePanel/etc.) — "type" color                   |
| `support.type.extension.lazuli`                                      | Extension types (Cell/Hook/Validator/etc.) — "type" color                       |
| `support.type.domain.lazuli`                                         | User-defined types (catch-all uppercase) — "type" color, optionally muted       |
| `support.function.type-constructor.lazuli`                           | Type constructor sugar (`many`/`list_of`/`ref`) — "support function" color      |
| `variable.other.field.lazuli`                                        | Field name in declarations — "variable/property" color                          |
| `variable.other.X.lazuli` family                                     | Various binding/dictionary/key sites — "variable/property" color                |
| `keyword.operator.X.lazuli`                                          | All operator families (logical/predicate/comparison/etc.) — "operator" color    |
| `constant.language.X.lazuli`                                         | Booleans, http methods, closed-catalog values — "constant.language" color       |
| `constant.numeric.lazuli`                                            | Number literals — "constant.numeric" / "number" color                           |
| `constant.other.X.lazuli`                                            | Enum members, locale tags, wildcards — "constant.other" color                   |
| `string.quoted.X.lazuli`                                             | String literals (and special-purpose strings) — "string" color                  |
| `comment.line.number-sign.lazuli`                                    | Comments — "comment" color                                                      |
| `punctuation.X.lazuli`                                               | Punctuation — usually "punctuation" or theme default                            |
| `storage.modifier.lazuli`                                            | Modifier words (required/optional/readonly/at/from/etc.) — "modifier" color     |
| `entity.name.namespace.lazurite.toml` (Lazurite.toml only)           | Known table headers (`[lazuli]`, `[lazurite]`, etc.) — "namespace" color        |
| `entity.name.tag.lazurite-target.toml` (Lazurite.toml only)          | Dotted suffix (`go` in `[generate.go]`) — "tag" color                           |
| `support.function.lazurite-key.toml` (Lazurite.toml only)            | Known keys (runtime/template/strict/etc.) — "function/key" color                |
| `entity.name.reference.plugin.lazuli` (Lazurite.toml only)           | `@plugin/X` module references — "decorator" / "reference" color                 |

## Full scope catalog

Scopes are grouped by family root. Within each family they're ordered
alphabetically.

### `comment.*`

#### `comment.line.number-sign.lazuli`
- **Role:** line comment introduced by `#`.
- **Used in:** main grammar, repository `comments`, included by every block pattern.
- **Suggested theme color:** the theme's "comment" color (usually italic muted gray).
- **Example:** `# this is a comment`

### `string.*`

#### `string.quoted.double.lazuli`
- **Role:** generic double-quoted string literal.
- **Used in:** main grammar, repository `strings`, included by every block pattern. Embeds `constant.character.escape.lazuli` for backslash escapes.
- **Suggested theme color:** standard "string" color.

#### `string.quoted.double.escape-route.lazuli`
- **Role:** the quoted body of a top-level `escape_route "..."` annotation.
- **Used in:** `statements-misc` (top level `escape_route` line).
- **Suggested theme color:** "string" color; consider an italic / softer variant if your theme distinguishes embedded annotation strings.

#### `string.quoted.double.rule-name.lazuli`
- **Role:** the quoted name of a `rule "..."` declaration inside a `policies`/`rules` context.
- **Used in:** `rule-block` (begin capture).
- **Suggested theme color:** "string" color; can be styled with a tiny accent (e.g. bold) since rule names act as identifiers.

#### `constant.character.escape.lazuli`
- **Role:** the `\X` inside a string literal.
- **Used in:** main grammar, inside `strings`.
- **Suggested theme color:** "constant.character.escape" color (often a contrasting hue inside the string).

### `keyword.control.*`

#### `keyword.control.declaration.structural.lazuli`
- **Role:** top-level kind keywords that introduce a major declaration. The headline "this thing is a feature/app/workspace/etc." word.
- **Used in:** `feature-decl`, `app-decl`, `workspace-decl`, `registry-decl`, `profile-decl`, `contract-block`, `experience-decl`, `route-decl`, `permission-decl`, `role-decl`, `plan-block`, `resource-block`, `record-block`, `enum-block`, `audience-block`, `aggregate-block`, `event-group-block`, `event-trace-block`, `workflow-block`, `operation-block`, `view-block`, `experience-block`, `secret-rotation-block`, `error-page-block`, `rule-block`, `poller-block`, `api-block`, `command-block`, `query-block`, `webhook-block`, `job-block`, `agent-block`, `notification-block`, `report-block`, `channel-block`, `tenant-migration-block`, `lifecycle-block`, `extends-block` (top-level), `auth-block`, `cache-block`, `translation-block`, `headers-block`, `cookie-block`, `proxy-block`, `limits-block`, `encryption-block`, `locale-block`, `logging-block`, `tracing-block`, `runtime-block`, `deploy-block`, `services-block`, `communication-block`, `urls-block`, `env-block`, `integrations-block`, `capabilities-block`, `packs-block`, `bindings-block`, `emits-block` (event sub), and `statements-misc` `escape_route`.
- **Suggested theme color:** the theme's "class declaration" / "type declaration" color (often blue or purple).
- **Example:** the `feature` in `feature item` or the `command` in `command create`.

#### `keyword.control.section.lazuli`
- **Role:** block-opener keywords that introduce a sub-block whose content has a different grammar (no name on the opener line).
- **Used in:** `surface-decl` (the `surface` keyword), `params-block`, `input-block`, `filters-block`, `scope-block`, `tests-block`, `policies-block`, `policy-fields-block`, `errors-block`, `emits-block`, `extensions-block`, `defaults-block`, `non-goals-block`, `invariants-block`, `composite-key-block`, `plan-block` (`features`/`limits`/`trial`), `bare-block-opener` (catch-all for known section-style keywords on a bare line).
- **Suggested theme color:** standard "keyword.control" color.
- **Examples:** `params`, `filters`, `input`, `audit`, `emits`, `tests`, `policies`, `errors`.

#### `keyword.control.statement.lazuli`
- **Role:** statement keywords inside named blocks — the verbs/qualifiers of the DSL.
- **Used in:** `auth-block` (identity/password/oauth/...), `cache-block` opener, `lifecycle-block` (state/transition/from/to/...), `invariants-block` (invariant/when/message), `aggregate-block` (root/contains/invariants), `workflow-block` (state/transition/...), `operation-block` (method/path/input/...), `view-block` (source/submit/columns/...), `experience-block` (imports/uses/anchor/...), `extends-sub-block` (block/platforms/audience/action), `secret-rotation-block` (cadence/overlap/auto_rollback), `error-page-block` (template/audience), `rule-block` (deny/forbid/where/message/when), `poller-block` (source/cursor/retry/...), `api-block` (method/path/policy/...), `command-block` (creates/updates/returns/route/let/target/emits/input/policy/...), `audit-block` (the `audit` opener and `emit_to`), `emits-sub-block` (the `emits` opener), `invalidates-block` (the `invalidates` opener), `approval-block` (the `approval` opener), `deprecated-sub-block` (the `deprecated` opener), `query-block` (returns/paginate/sql/order/search/...), `webhook-block` (path/payload/verify/...), `verify-block` (the `verify` opener), `replay-block` (the `replay` opener), `dlq-block` (the `dlq` opener), `job-block` (queue/trigger/let/updates/calls/...), `agent-block` (input/context/policy/output/...), `tools-block` (the `tools` opener), `expose-http-block` (the `expose http` opener), `evals-block` (the `evals` opener and `case`/`requires`/`forbids`/...), `notification-block` (channel/recipient/trigger/...), `digest-block` (the `digest` opener), `throttle-block` (the `throttle` opener), `report-block` (source/columns/formats/...), `columns-block` (the `columns` opener), `channel-block` (tenant_from/policy/...), `tenant-migration-block` (target/axis/idempotency/retry/...), `extends-sub-block` `extends`/`slot`, `extends-block` `slot`, `view-block` `route`/`action`, `statements-misc` `subscription`/`gate`/`behind`/`quota`.
- **Suggested theme color:** standard "keyword" or "keyword.control" color.

### `keyword.operator.*`

#### `keyword.operator.assignment.lazuli`
- **Role:** the `=` in `let X = …`.
- **Used in:** `command-block` `let` capture, `job-block` `let` capture.
- **Suggested theme color:** "operator" color.

#### `keyword.operator.comparison.lazuli`
- **Role:** comparison and equality operators (`==`, `!=`, `<=`, `>=`, `<`, `>`, `=`).
- **Used in:** `operators` repository, included in any expression context.
- **Suggested theme color:** "operator" color.

#### `keyword.operator.containment.lazuli`
- **Role:** PostgreSQL-style JSON containment operators (`@>`, `<@`, `?|`, `?&`).
- **Used in:** `filters-block` and `operators` repository.
- **Suggested theme color:** "operator" color; theme can give these a distinct accent if desired.

#### `keyword.operator.logical.lazuli`
- **Role:** logical / quantifier connectives (`and`, `or`, `not`, `AND`, `OR`, `has`).
- **Used in:** `filters-block`, `tests-block`, `policies-block`, `invariants-block`, `rule-block`.
- **Suggested theme color:** "operator" or "keyword.operator.logical" color.

#### `keyword.operator.plan-and-gate.lazuli`
- **Role:** the `plan.feature` / `plan.limit` accessor used in gating expressions.
- **Used in:** `statements-misc`.
- **Suggested theme color:** "operator" color; consider a subtle accent since this is a load-bearing DSL phrase.

#### `keyword.operator.predicate.lazuli`
- **Role:** filter predicates (`when`, `in`, `has`, `exists`, `matches`, `is`, `between`, `not`).
- **Used in:** `filters-block`.
- **Suggested theme color:** "operator" color.

#### `keyword.operator.transition.lazuli`
- **Role:** the `->` arrow used in state transitions and view actions.
- **Used in:** `view-block` `action` capture and `operators` repository.
- **Suggested theme color:** "operator" color, ideally bold for emphasis.

#### `keyword.operator.union.lazuli`
- **Role:** the `|` union operator used in union types.
- **Used in:** `operators` repository.
- **Suggested theme color:** "operator" color.

### `keyword.other.*`

#### `keyword.other.plan.lazuli`
- **Role:** plan-block specific keywords that aren't statement keywords (`then`, `unlimited`).
- **Used in:** `plan-block`.
- **Suggested theme color:** "keyword" color.

### `entity.name.type.*` (declared entities)

The `entity.name.type.X.lazuli` family marks the *name* of a declared
top-level entity. Each variant exists so themes can differentiate (or
not) between feature/app/aggregate/etc. names.

#### `entity.name.type.aggregate.lazuli`
- **Role:** name of an `aggregate Foo` declaration.
- **Used in:** `aggregate-block`.

#### `entity.name.type.app.lazuli`
- **Role:** name of an `app Foo` declaration.
- **Used in:** `app-decl`.

#### `entity.name.type.contract.lazuli`
- **Role:** name of a `contract Foo` declaration.
- **Used in:** `contract-block`.

#### `entity.name.type.enum.lazuli`
- **Role:** name of an `enum Foo` declaration.
- **Used in:** `enum-block`.

#### `entity.name.type.experience.lazuli`
- **Role:** name of an `experience foo` declaration.
- **Used in:** `experience-decl`, `experience-block`.

#### `entity.name.type.feature.lazuli`
- **Role:** name of a `feature Foo` declaration.
- **Used in:** `feature-decl`.

#### `entity.name.type.plan.lazuli`
- **Role:** name of a `plan foo` declaration.
- **Used in:** `plan-block`.

#### `entity.name.type.profile.lazuli`
- **Role:** name of a `profile Foo` declaration.
- **Used in:** `profile-decl`.

#### `entity.name.type.record.lazuli`
- **Role:** name of a `record Foo` declaration.
- **Used in:** `record-block`.

#### `entity.name.type.resource.lazuli`
- **Role:** name of a `resource|aggregate|entity Foo` declaration at indent 4 inside a feature.
- **Used in:** `resource-block`.

#### `entity.name.type.workspace.lazuli`
- **Role:** name of a `workspace Foo` declaration.
- **Used in:** `workspace-decl`.

- **Suggested theme color (for all of the above):** the theme's "type name" / "class name" color. Themes can collapse all variants to one color.

### `entity.name.function.*` (named blocks + statement keywords + named events / lifecycles / etc.)

This is the largest family. Two sub-roles share the prefix:

1. **`entity.name.function.named-block.lazuli`** — the *name* of a generic
   named block (`command X`, `query.list X`, `webhook X`, `job X`, etc.).
   Themes should color this like a function name.
2. **`entity.name.function.statement.*.lazuli`** — sub-block statement
   keywords (e.g. the `csp` line inside `headers`). These act like
   keywords-with-namespace (the prefix tells you which block they belong
   to). Themes can either treat them as plain keywords or give them a
   distinct accent.

#### `entity.name.function.named-block.lazuli`
- **Role:** name of any generic named block (the `X` in `command X`, `query.list X`, `webhook X`, `job X`, `agent X`, `notification X`, `report X`, `channel X`, `tenant_migration X`, `workflow X`, `operation X`, `poller X`, `api X`).
- **Used in:** `api-block`, `command-block`, `query-block`, `webhook-block`, `job-block`, `agent-block`, `notification-block`, `report-block`, `channel-block`, `tenant-migration-block`, `workflow-block`, `operation-block`, `poller-block`.
- **Suggested theme color:** "function name" color.

#### `entity.name.function.action.lazuli`
- **Role:** name of an action inside a `view` block (`action send -> CommandFoo`).
- **Used in:** `view-block`.
- **Suggested theme color:** "function name" or "method name" color.

#### `entity.name.function.cache-profile.lazuli`
- **Role:** name of a named cache profile (`cache short_ttl`).
- **Used in:** `cache-block` opener.
- **Suggested theme color:** "function name" color.

#### `entity.name.function.eval-case.lazuli`
- **Role:** name of an evaluation case inside an `agent.evals` block.
- **Used in:** `evals-block`.
- **Suggested theme color:** "function name" color.

#### `entity.name.function.event-group.lazuli`
- **Role:** name of an `event_group foo` declaration (also matches `event_group foo.*`).
- **Used in:** `event-group-block`.
- **Suggested theme color:** "function name" color.

#### `entity.name.function.event.lazuli`
- **Role:** name of an event — both at declaration sites (`event sent`) and at emission sites (`emits event_name`).
- **Used in:** `emits-block`, `emits-sub-block`, `event-group-block` (`event` / `event.trace` lines), `event-trace-block`, `command-block` `emits` capture.
- **Suggested theme color:** "function name" color (or "event name" if your theme distinguishes).

#### `entity.name.function.extension-point.lazuli`
- **Role:** name of an extension point (the `X` in `fn X:`, `hook X:`, `validator X:`, `adapter X:`, `query_modifier X:`, `client X:`, `server X:`, `block X:`).
- **Used in:** `typed-extensions`.
- **Suggested theme color:** "function name" color.

#### `entity.name.function.lifecycle.lazuli`
- **Role:** name of a `lifecycle foo` declaration on a resource.
- **Used in:** `lifecycle-block`.
- **Suggested theme color:** "function name" color.

#### `entity.name.function.route.lazuli`
- **Role:** name of a top-level `route foo` declaration.
- **Used in:** `route-decl`.
- **Suggested theme color:** "function name" color.

#### `entity.name.function.secret-rotation.lazuli`
- **Role:** name of a `secret_rotation foo` declaration.
- **Used in:** `secret-rotation-block`.
- **Suggested theme color:** "function name" color.

#### `entity.name.function.view.lazuli`
- **Role:** name of a `view foo` declaration (the lowercase view name; the optional uppercase Type after it is colored as `support.type.ui.lazuli`).
- **Used in:** `view-block`.
- **Suggested theme color:** "function name" color.

#### Statement-keyword sub-family

Each scope below marks the *statement keywords* that live inside its
named block. The block name appears in the leaf for clarity and so
themes can target a single block if desired (e.g. dim cookie keys
specifically). All of them share the same default coloring guidance:
**use your "keyword.control" or "keyword" color**.

| Scope                                                    | Block / context                                                                                                          | Example keywords                                                                                            |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------- |
| `entity.name.function.statement.app-meta.lazuli`         | `statements-app-meta` — top-level app metadata lines                                                                     | `title`, `version`, `default_locale`, `default_timezone`, `auth_failed_redirect`, `not_found`, `mode`, `service_ready`, `enforce_service_boundaries`, `environment` |
| `entity.name.function.statement.approval.lazuli`         | `approval-block`                                                                                                         | `required_when`, `by`, `timeout`, `then`, `deny`, `allow`                                                   |
| `entity.name.function.statement.audit.lazuli`            | `audit-block`                                                                                                            | `emit_to`                                                                                                   |
| `entity.name.function.statement.cache.lazuli`            | `cache-block`                                                                                                            | `key`, `ttl`, `tags`, `namespace`, `stale_while_revalidate`, `coalesce`, `sliding`                          |
| `entity.name.function.statement.columns.lazuli`          | `columns-block` (inside `report`)                                                                                        | `from`, `label`, `format`                                                                                   |
| `entity.name.function.statement.communication.lazuli`    | `communication-block`                                                                                                    | `internal`, `external`, `async`, `sync`, `propagate`, `timeout`                                             |
| `entity.name.function.statement.contract.lazuli`         | `statements-contract` (inside `contract`)                                                                                | `compatibility`, `import`, `version`, `provides`, `transport`                                               |
| `entity.name.function.statement.cookie.lazuli`           | `cookie-block`                                                                                                           | `default`, `session`, `csrf`, `signed`, `secure`, `http_only`, `same_site`, `max_age`, `domain`, `path`     |
| `entity.name.function.statement.defaults.lazuli`         | `defaults-block`                                                                                                         | `tenancy`, `timestamps`, `no_timestamps`, `soft_delete`, `retention`, `policy_for`                          |
| `entity.name.function.statement.deploy.lazuli`           | `deploy-block`                                                                                                           | `migrations`, `migration_lock`, `destructive_migrations`, `rollback`, `topology`, `environment`, `strategy`, `lock_timeout`, `pre_migration_hook`, `post_migration_hook`, `checkpoint` |
| `entity.name.function.statement.deprecated.lazuli`       | `deprecated-sub-block`                                                                                                   | `since`, `replacement`, `sunset`                                                                            |
| `entity.name.function.statement.digest.lazuli`           | `digest-block`                                                                                                           | `every`, `group_by`, `max_size`, `template_strategy`                                                        |
| `entity.name.function.statement.encryption.lazuli`       | `encryption-block`                                                                                                       | `key`, `source`, `algorithm`, `rotation`, `rotation_profile`                                                |
| `entity.name.function.statement.env.lazuli`              | `env-block`                                                                                                              | `group`, `client`, `server`, `required`, `optional`, `default`                                              |
| `entity.name.function.statement.errors.lazuli`           | `errors-block`                                                                                                           | `default`, `expose`, `hide`, `client`, `server`, `message`, `status`, `code`                                |
| `entity.name.function.statement.event-group.lazuli`      | `event-group-block`                                                                                                      | `payload`, `when`, `level`                                                                                  |
| `entity.name.function.statement.event-trace.lazuli`      | `event-trace-block`                                                                                                      | `level`                                                                                                     |
| `entity.name.function.statement.event.lazuli`            | `emits-block` (event sub-statements)                                                                                     | `payload`, `payload_group`                                                                                  |
| `entity.name.function.statement.extension.lazuli`        | `typed-extensions` — the leading kind (`fn`/`hook`/`validator`/etc.)                                                     | `client`, `fn`, `hook`, `validator`, `adapter`, `query_modifier`, `server`, `block`                         |
| `entity.name.function.statement.feature-meta.lazuli`     | `statements-feature-meta` — feature-scoped meta lines                                                                    | `purpose`, `context`, `imports`, `uses`, `requires`, `target`, `targets`, `to`, `tenant`, `tenant_from`, `reason`, `trigger`, `schedule`, `queue`, `owner`, ... |
| `entity.name.function.statement.headers.lazuli`          | `headers-block`                                                                                                          | `csp`, `hsts`, `x_frame_options`, `x_content_type_options`, `referrer_policy`, `permissions_policy`, `max_age`, `include_subdomains`, `preload` |
| `entity.name.function.statement.integration.lazuli`      | `integrations-block`                                                                                                     | `adapter`, `credentials`, `environment`, `data_classification`, `operation`, `contract`                     |
| `entity.name.function.statement.limits.lazuli`           | `limits-block`                                                                                                           | `body_size`, `header_size`, `upload_size`, `timeout`                                                        |
| `entity.name.function.statement.locale.lazuli`           | `locale-block`                                                                                                           | `default`, `supported`, `fallback`                                                                          |
| `entity.name.function.statement.logging.lazuli`          | `logging-block`                                                                                                          | `level`, `format`, `redact`, `sample_rate`                                                                  |
| `entity.name.function.statement.non-goals.lazuli`        | `non-goals-block`                                                                                                        | `delegated_to`, `out_of_scope`, `constraints`                                                               |
| `entity.name.function.statement.packs.lazuli`            | `packs-block`                                                                                                            | `provides`, `from`, `feature`                                                                               |
| `entity.name.function.statement.policy.lazuli`           | `policies-block`, `policy-fields-block`                                                                                  | `read`, `write`, `create`, `update`, `delete`, plus user-named policy actions                               |
| `entity.name.function.statement.proxy.lazuli`            | `proxy-block`                                                                                                            | `trusted`, `real_ip_header`, `forwarded_proto_header`, `forwarded_host_header`                              |
| `entity.name.function.statement.replay.lazuli`           | `replay-block` (inside `webhook`)                                                                                        | `allow`, `deny`, `within`, `dedupe`, `by`                                                                   |
| `entity.name.function.statement.resource.lazuli`         | `resource-block`, `lock-block`, `composite-key-block`, `statements-resource`                                             | `tenancy`, `soft_delete`, `timestamps`, `no_timestamps`, `retention`, `validates`, `validate`, `unique`, `index`, `on_delete`, `derived`, `has_many`, `inverse`, `previously`, `migrated`, `alias`, `composite_key`, `lock`, `invariant`, `invariants`, `fields`, `primary` |
| `entity.name.function.statement.runtime.lazuli`          | `runtime-block`                                                                                                          | `unit`, `serves`, `runs`, `healthcheck`, `readiness`                                                        |
| `entity.name.function.statement.scope.lazuli`            | `scope-block`                                                                                                            | `reason`                                                                                                    |
| `entity.name.function.statement.services.lazuli`         | `services-block`                                                                                                         | `service`, `owns`, `exposes`, `publishes`, `consumes`                                                       |
| `entity.name.function.statement.tests.lazuli`            | `tests-block`                                                                                                            | `allows`, `denies`, `permits`, `forbids`, `accepted`, `rejected`, `case`, `requires`, `golden`, `min_score`, `when`, `by`, `from`, `as`, `to` |
| `entity.name.function.statement.throttle.lazuli`         | `throttle-block`                                                                                                         | `max_per`, `per_recipient`, `per_channel`, `burst`                                                          |
| `entity.name.function.statement.tracing.lazuli`          | `tracing-block`                                                                                                          | `propagate`, `sample_rate`, `exporter`                                                                      |
| `entity.name.function.statement.translation.lazuli`      | `translation-block`                                                                                                      | `catalog`, `key`, `plural`                                                                                  |

### `entity.name.label.*` (identifier labels)

#### `entity.name.label.anchor-target.lazuli`
- **Role:** the target identifier in `slot foo after bar` (the `bar`).
- **Used in:** `extends-sub-block`.
- **Suggested theme color:** "label" / "tag" color.

#### `entity.name.label.audience.lazuli`
- **Role:** name of an `audience foo` block opener.
- **Used in:** `audience-block`.
- **Suggested theme color:** "label" / "tag" color.

#### `entity.name.label.integration.lazuli`
- **Role:** name of a registry `integration foo` line.
- **Used in:** `integrations-block`.
- **Suggested theme color:** "label" / "tag" color.

#### `entity.name.label.rbac.lazuli`
- **Role:** name of a `permission foo:bar` or `role foo` declaration.
- **Used in:** `permission-decl`, `role-decl`.
- **Suggested theme color:** "label" color.

#### `entity.name.label.slot.lazuli`
- **Role:** name of a `slot foo` line inside an `extends` context.
- **Used in:** `extends-sub-block`, `extends-block`.
- **Suggested theme color:** "label" color.

#### `entity.name.label.surface-area.lazuli`
- **Role:** the optional second identifier on a `surface foo bar` line (the `bar`).
- **Used in:** `surface-decl`.
- **Suggested theme color:** "label" color.

#### `entity.name.label.surface-target.lazuli`
- **Role:** the first identifier on a `surface foo` line.
- **Used in:** `surface-decl`.
- **Suggested theme color:** "label" color.

### `entity.name.reference.*` (cross-references)

#### `entity.name.reference.anchor.lazuli`
- **Role:** an `@anchor.foo` reference on an `extends` line.
- **Used in:** `extends-sub-block`, `extends-block`.
- **Suggested theme color:** "namespace" or "decorator" color.

#### `entity.name.reference.decorator.lazuli`
- **Role:** any dotted decorator-style reference matching `@foo.bar` (catch-all, lower priority than `entity.name.reference.package` and the explicit decorator catalog in `entity.name.tag.decorator`).
- **Used in:** `references` repository.
- **Suggested theme color:** "decorator" / "tag" color.

#### `entity.name.reference.model-path.lazuli`
- **Role:** dotted paths starting with an uppercase letter (`Resource.field`, `Item.tags`).
- **Used in:** `references` repository.
- **Suggested theme color:** "type" color (slightly muted) or "namespace".

#### `entity.name.reference.package.lazuli`
- **Role:** package import references `@runtime/foo` and `@plugin/foo`.
- **Used in:** `references` repository (highest priority of the `entity.name.reference.*` family).
- **Suggested theme color:** "namespace" / "package" color. Distinct from decorators.

#### `entity.name.reference.semantic.lazuli`
- **Role:** dotted paths starting with a lowercase letter (`item.title`, `params.id`). The unscoped lowercase counterpart of `model-path`.
- **Used in:** `references` repository.
- **Suggested theme color:** "variable.other" / "property" color. Optionally muted.

### `entity.name.tag.*`

#### `entity.name.tag.decorator.lazuli`
- **Role:** the curated decorator catalog (`@semantic`, `@cap`, `@pii`, `@key`, `@slug`, `@full_text`, `@llm`, `@tool`, `@adapter`, `@policy`, `@scope`, `@role`, `@actor`, `@anchor`, `@client`, `@fn`, `@hook`, `@validator`, `@query_modifier`, `@translation`), with optional `.subname` and optional `(...)` argument list.
- **Used in:** `decorators` repository.
- **Suggested theme color:** "decorator" / "tag" color (often a distinct hue like teal or yellow).

### `entity.name.audit.fields.lazuli`
- **Role:** the field-list / `none` / `default` payload that follows `audit` on an `audit ...` line opener.
- **Used in:** `audit-block` (begin capture, group 3).
- **Note:** This is the only scope in the grammar that does NOT follow the `entity.name.{type,function,label,reference,tag,namespace,...}` convention — the leaf `audit` is a custom subnamespace. Worth refactoring to `variable.other.audit-fields.lazuli` or `entity.name.label.audit-fields.lazuli` for consistency.
- **Suggested theme color:** "variable.other" or "string" color.

### `support.type.*` (built-in language types)

#### `support.type.primitive.lazuli`
- **Role:** built-in scalar types (`ID`, `Text`, `String`, `Email`, `Url`, `Boolean`, `Bool`, `Int`, `Integer`, `Float`, `Decimal`, `Numeric`, `Money`, `Date`, `DateTime`, `Json`, `JSON`, `File`, `Secret`, `Hashed`, `Encrypted`, `E2ee`, `Token`, `Slug`, `Uuid`, `Phone`, `Currency`, `GeoPoint`), optionally prefixed by a `@semantic.|@cap.|@pii.|@key.` decorator.
- **Used in:** `types` repository.
- **Suggested theme color:** "type" color, ideally bold or colored more saturated than user-defined types.

#### `support.type.ui.lazuli`
- **Role:** UI component types (`Form`, `AuthForm`, `Mutation`, `Transition`, `Table`, `List`, `CardList`, `Screen`, `SidePanel`, `Sheet`, `Terminal`, `Drawer`, `Dashboard`, `Wizard`).
- **Used in:** `types` repository, also explicitly used in `view-block` opener (capture 4) and `policy-fields-block`.
- **Suggested theme color:** "type" color; consider a subtly different hue from `primitive` if your theme supports it.

#### `support.type.extension.lazuli`
- **Role:** extension-point types (`Cell`, `Block`, `CellRenderer`, `Hook`, `Function`, `Validator`, `QueryModifier`, `WorkflowEffect`, `PageBlock`, `ViewBlock`, `FormField`, `IntegrationAdapter`).
- **Used in:** `types` repository.
- **Suggested theme color:** "type" color.

#### `support.type.domain.lazuli`
- **Role:** catch-all for any uppercase identifier that isn't a primitive/UI/extension type.
- **Used in:** `types` repository (lowest-priority within `#types`); also explicitly emitted in `command-block` `creates|updates|deletes` and `returns` captures, in `job-block` `updates|creates|deletes` capture, in `policy-fields-block` `fields Foo`, in `event-group-block` `event_group foo on Foo`.
- **Suggested theme color:** "type" color, optionally slightly muted to distinguish from built-in primitives.

#### `support.function.type-constructor.lazuli`
- **Role:** type constructor sugars (`many`, `list_of`, `ref`).
- **Used in:** `statements-misc`.
- **Suggested theme color:** "support.function" color or "keyword".

### `variable.other.*`

#### `variable.other.binding.lazuli`
- **Role:** the bound name in `let X = …`.
- **Used in:** `command-block`, `job-block`.
- **Suggested theme color:** "variable" / "parameter" color.

#### `variable.other.dictionary-key.lazuli`
- **Role:** map-style keys in `non_goals` (and similar dictionary contexts).
- **Used in:** `non-goals-block`.
- **Suggested theme color:** "property" / "key" color.

#### `variable.other.field.lazuli`
- **Role:** the field name on the LHS of a `name: Type` field declaration.
- **Used in:** `field-decl` (matched in any block that includes `field-decl`).
- **Suggested theme color:** "property" / "variable.declaration" color.

#### `variable.other.query-key.lazuli`
- **Role:** the field name in a `query.lookup` `by foo: Type` line.
- **Used in:** `query-block`.
- **Suggested theme color:** "property" / "key" color.

#### `variable.other.route-slot.lazuli`
- **Role:** the slot name on a `route foo: Type` line inside `view`/`api`/`command` blocks.
- **Used in:** `view-block`, `api-block`, `command-block`.
- **Suggested theme color:** "parameter" color.

#### `variable.other.search-target.lazuli`
- **Role:** the dotted target of a `search params.x over …` line.
- **Used in:** `query-block`.
- **Suggested theme color:** "variable" / "property" color.

#### `variable.other.url-key.lazuli`
- **Role:** key names inside a top-level `urls` block.
- **Used in:** `urls-block`.
- **Suggested theme color:** "property" color.

### `constant.language.*` (closed-catalog values)

All of these mark words drawn from a finite enum and should color
identically to standard "constant.language" tokens.

| Scope                                            | Values                                                                  | Used in                                |
| ------------------------------------------------ | ----------------------------------------------------------------------- | -------------------------------------- |
| `constant.language.boolean.lazuli`               | `true`, `false`, `nil`, `null`                                          | `constants` repository                 |
| `constant.language.channel.lazuli`               | `email`, `push`, `sms`, `in_app`                                        | `notification-block`                   |
| `constant.language.cookie.lazuli`                | `lax`, `strict`, `none`                                                 | `cookie-block`                         |
| `constant.language.deploy.lazuli`                | `rolling`, `blue_green`, `canary`                                       | `deploy-block`                         |
| `constant.language.dlq.lazuli`                   | `emit`, `drop`, `handler`                                               | `dlq-block`                            |
| `constant.language.http-method.lazuli`           | `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, `HEAD`              | `api-block`, `expose-http-block`       |
| `constant.language.lock.lazuli`                  | `optimistic`, `pessimistic`, `row_level`                                | `lock-block`                           |
| `constant.language.log-level.lazuli`             | `debug`, `info`, `warn`, `error`, `json`, `text`, `pii`, `none`         | `logging-block`                        |
| `constant.language.report-format.lazuli`         | `csv`, `xlsx`                                                           | `report-block`                         |
| `constant.language.rotation.lazuli`              | `manual`, `kms_managed`                                                 | `encryption-block`                     |
| `constant.language.template-strategy.lazuli`     | `merge`, `append`                                                       | `digest-block`                         |
| `constant.language.transport.lazuli`             | `http`                                                                  | `expose-http-block` opener             |
| `constant.language.verify.lazuli`                | `hmac`, `jwt`, `none`                                                   | `verify-block`                         |
| `constant.language.visibility.lazuli`            | `public`, `private`, `signed`                                           | `report-block`                         |

- **Suggested theme color:** "constant.language" color.

### `constant.numeric.*`

#### `constant.numeric.lazuli`
- **Role:** generic number literal (integer or decimal).
- **Used in:** `constants` repository.
- **Suggested theme color:** "constant.numeric" / "number" color.

#### `constant.numeric.http-status.lazuli`
- **Role:** the `404`/`500`/etc. on an `error_page <code>` line.
- **Used in:** `error-page-block`.
- **Suggested theme color:** "number" color; can be styled bold for emphasis.

### `constant.other.*`

#### `constant.other.direction.lazuli`
- **Role:** sort direction tokens (`asc`, `desc`).
- **Used in:** `query-block`.
- **Suggested theme color:** "constant" color.

#### `constant.other.enum-member.lazuli`
- **Role:** a bare identifier that is a member inside an `enum` body.
- **Used in:** `enum-block`.
- **Suggested theme color:** "constant" color, optionally bolder than other `constant.other` variants.

#### `constant.other.locale-tag.lazuli`
- **Role:** BCP-47–style locale tags (`en-US`, `pt-BR`, etc.) inside locale/translation contexts.
- **Used in:** `translation-block`, `locale-block`.
- **Suggested theme color:** "constant" / "tag" color.

#### `constant.other.verify-alg.lazuli`
- **Role:** the optional algorithm name on a `verify hmac sha256`-style line.
- **Used in:** `verify-block`.
- **Suggested theme color:** "constant" color.

#### `constant.other.wildcard.lazuli`
- **Role:** the bare `*` wildcard token.
- **Used in:** `constants` repository.
- **Suggested theme color:** "constant" color.

### `storage.modifier.lazuli`
- **Role:** modifier words attached to a declaration. Catalog: `required`, `optional`, `default`, `readonly`, `raw`, `override`, `previously`, `migrated`, `alias`, `per`, `at`, `from`, `provides`, `cascade`, `restrict`, `nullify`, `by`, `on_delete`, `inverse`, `primary`, `terminal`, `initial`, `external`, `internal`, `sync`, `async`. Also emitted explicitly in:
  - `surface-decl` `override` capture
  - `extensions-block` `at` keyword
  - `event-group-block` `on` connector
  - `extends-sub-block` `after`/`before`
  - `command-block`/`job-block` `from` connector
  - `agent-block` `output stream|discriminator`
- **Suggested theme color:** "storage.modifier" color (often italic).

### `punctuation.*`

#### `punctuation.accessor.lazuli`
- **Role:** the `.` accessor between segments of a path.
- **Used in:** `punctuation` repository.
- **Suggested theme color:** "punctuation" color (theme default usually).

#### `punctuation.definition.generic.begin.lazuli` / `.end.lazuli`
- **Role:** `[` and `]` delimiters.
- **Used in:** `punctuation` repository.
- **Suggested theme color:** "punctuation" color.

#### `punctuation.section.parens.begin.lazuli` / `.end.lazuli`
- **Role:** `(` and `)` delimiters.
- **Used in:** `punctuation` repository.
- **Suggested theme color:** "punctuation" color.

#### `punctuation.separator.comma.lazuli`
- **Role:** comma separator.
- **Used in:** `punctuation` repository.
- **Suggested theme color:** "punctuation" color.

#### `punctuation.separator.key-value.lazuli`
- **Role:** colon used as a map-key separator (`policy.read: …`).
- **Used in:** `policies-block`, `policy-fields-block`, `non-goals-block`, `punctuation` repository.
- **Suggested theme color:** "punctuation" color.

#### `punctuation.separator.type.lazuli`
- **Role:** colon used as the type-annotation separator (`name: Type`).
- **Used in:** `field-decl`, `view-block` (route slot), `api-block` (route slot), `command-block` (route slot), `query-block` (`by` capture), `typed-extensions`.
- **Suggested theme color:** "punctuation" color. Themes can give this a slightly different accent from `key-value` if desired (it carries type-annotation semantics).

## Lazurite manifest grammar (Lazurite.toml)

The TOML overlay only adds a handful of scopes on top of the standard
TOML grammar (`source.toml`); everything else falls through to the
host-provided TOML grammar.

#### `entity.name.namespace.lazurite.toml`
- **Role:** known top-level table headers — `[lazuli]`, `[lazurite]`, `[plugins]`, `[project]`, `[migrations]`, `[seeds]`, `[dev]`, `[runtime]`, `[targets]`, plus the namespace part of dotted headers `[generate.X]` and `[frontends.X]`.
- **Suggested theme color:** "namespace" / "type" color.

#### `entity.name.tag.lazurite-target.toml`
- **Role:** the dotted suffix on `[generate.X]` and `[frontends.X]` headers — the `go`/`ts` in `[generate.go]` / `[generate.ts]`, or any user-named frontend in `[frontends.web]`.
- **Suggested theme color:** "tag" / "label" color.

#### `support.function.lazurite-key.toml`
- **Role:** known Lazurite manifest keys (`name`, `module`, `schema`, `runtime`, `template`, `template_version`, `app_dir`, `out`, `gofmt`, `strict`, `emit_main`, `submodule`, `dev_replace`, `target`, `source`, `audiences`, `generated`, `manual`, `strategy`, `dir`, `auto`, `plugin_paths`, `version`, `path`).
- **Suggested theme color:** "function name" / "key" color, or "keyword" if you want manifest keys to read like keywords.

#### `entity.name.reference.plugin.lazuli`
- **Role:** a `@plugin/foo` quoted-string key inside a `[plugins]` table.
- **Suggested theme color:** "namespace" / "decorator" color, ideally aligned with `entity.name.reference.package.lazuli` so the same import looks the same in `.lzi` and `Lazurite.toml`.

#### Standard TOML scopes also emitted

- `punctuation.definition.table.toml` (the `[` / `]` around table headers)
- `punctuation.accessor.toml` (the `.` between dotted-table parts)
- `punctuation.definition.string.begin.toml` / `.end.toml` (the `"` around plugin keys)

These follow standard TOML conventions and will color correctly under
any theme that already supports TOML.

## Scope naming conventions used

- Top-level family roots follow VS Code/TextMate convention:
  - `keyword.control.X` — control-flow / declaration keywords
  - `keyword.operator.X` — operators
  - `entity.name.X` — declaring an entity (function, class, type, namespace, label, tag)
  - `entity.name.reference.X` — references to entities declared elsewhere
  - `support.type.X` / `support.function.X` — built-in language constructs
  - `variable.other.X` — user-defined identifiers
  - `constant.language.X` / `constant.numeric.X` / `constant.other.X` — literals
  - `storage.modifier` — modifier keywords
  - `punctuation.X` — separators and brackets
  - `comment.X` / `string.X` — standard
- All scopes end with `.lazuli` (or `.toml` for the Lazurite manifest
  overlay scopes that target standard TOML coloring families) so themes
  can target Lazuli specifically with a `source.lazuli` selector.
- Sub-leaves were intentionally collapsed (single-character role,
  single color) where multiple block-specific scopes would have given
  no extra theming value. The `entity.name.function.statement.X.lazuli`
  family is the deliberate exception: the per-block leaf is preserved so
  themes that want to dim or accent a specific block (e.g. cookie keys)
  can do so without affecting other statement keywords.

## Theme compatibility notes

- **Default Dark+ / Light+:** scopes color correctly without
  theme-specific tweaks — every scope is grounded in a standard
  TextMate root family that VS Code's defaults already cover.
- **Material Theme / Dracula / One Dark Pro:** scopes color correctly.
- **Solarized:** the `entity.name.tag.decorator.lazuli` scope used for
  decorators may render less prominently than ideal; consider an
  explicit color rule if decorators feel washed out.
- **Custom themes:** use the "Quick reference" table above as a
  starting point. The `entity.name.function.statement.X.lazuli` family
  can be coloured uniformly via the prefix
  `entity.name.function.statement` if per-block accents aren't needed.

## Inspection tip

In VS Code: `Ctrl+Shift+P` → "Developer: Inspect Editor Tokens and
Scopes" → click any token to see the assigned scope chain. Use this to
verify a token has the scope you expect — particularly useful when
debugging why a keyword is coloring like a type or vice versa.
