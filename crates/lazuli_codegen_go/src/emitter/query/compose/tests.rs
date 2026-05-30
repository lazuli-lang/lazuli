//! Cell W5 — `query.compose` codegen tests. The load-bearing case
//! (`compose_chat_inbox_emits_joins_subselects_and_tenant_predicate`) models a
//! real Hostpoint read (`messaging.list_chat_inbox`): a tenant-scoped root +
//! an FK JOIN + a `count` subselect + a `latest` subselect. It asserts the
//! generated SELECT carries the JOIN, BOTH correlated subselects, AND the
//! generated tenant predicate in the root `WHERE` — the verifiability proof
//! that scope inheritance reaches the emitted SQL (§5.2).

use super::super::test_support::{base_feature, emit, field, qname, resource};
use lazuli_ir::{
    AggFn, BuiltinType, ComposeJoin, ComposeProjection, ComposeQuery, ComposeScopeOrigin,
    ComposeSubselect, CompareOp, Expr, FkPath, KeyClause, OrderBy, OrderDir, Path, Predicate,
    PolicyRef, ProjectionSource, Query, SubselectKind, Tenancy, TypeRef,
};

/// Assert a column-aligned struct field row carries the given name, Go type,
/// and `db`/`json` tag, tolerating the inter-column alignment padding
/// `aligned_struct_rows` inserts. Matches a line that has `<name>`, then
/// `<go_type>`, then the exact tag, left-to-right.
fn field_row(out: &str, name: &str, go_type: &str, col: &str) -> bool {
    let tag = format!("`db:\"{col}\" json:\"{col}\"`");
    out.lines().any(|line| {
        let trimmed = line.trim_start();
        let Some(rest) = trimmed.strip_prefix(name) else {
            return false;
        };
        // Guard against prefix collisions (e.g. `Chat` vs `ChatID`): the char
        // after the name must be whitespace.
        if !rest.starts_with(char::is_whitespace) {
            return false;
        }
        let rest = rest.trim_start();
        let Some(after_ty) = rest.strip_prefix(go_type) else {
            return false;
        };
        after_ty.trim_start() == tag
    })
}

/// Build the `messaging` feature with the `chat_inbox` compose modeled on
/// `messaging.list_chat_inbox` (root Chat, join to counterpart user, unread
/// COUNT + last-message preview subselects), tenant-scoped + soft-deleted.
fn chat_inbox_feature() -> lazuli_ir::Feature {
    let mut feature = base_feature("messaging");

    feature.resources.push(resource("User", Vec::new()));

    // Chat — tenancy Org + soft delete, with a `counterpart: User` FK.
    let mut chat = resource(
        "Chat",
        vec![field("counterpart", TypeRef::UserDefined(qname("User")), true)],
    );
    chat.tenancy = Some(Tenancy::Org);
    chat.soft_delete = true;
    feature.resources.push(chat);

    // ChatMessage — the subselect child resource.
    feature.resources.push(resource(
        "ChatMessage",
        vec![
            field("chat", TypeRef::UserDefined(qname("Chat")), true),
            field("body", TypeRef::Builtin(BuiltinType::Text), true),
            field("sender", TypeRef::UserDefined(qname("User")), true),
            field("read_at", TypeRef::Builtin(BuiltinType::DateTime), false),
            field("created_at", TypeRef::Builtin(BuiltinType::DateTime), true),
        ],
    ));

    feature.queries.push(Query::Compose(ComposeQuery {
        name: "chat_inbox".to_owned(),
        public_contract: None,
        root: TypeRef::UserDefined(qname("Chat")),
        params: Vec::new(),
        joins: vec![ComposeJoin {
            // `chat.counterpart` — belongs-to FK from Chat onto User.
            path: FkPath::from_segments(["chat", "counterpart"]),
            alias: "cp".to_owned(),
            nullable: true,
        }],
        projections: vec![
            ComposeProjection {
                name: "chat_id".to_owned(),
                source: ProjectionSource::SelfCol("id".to_owned()),
            },
            ComposeProjection {
                name: "counterpart_name".to_owned(),
                source: ProjectionSource::Joined("cp".to_owned(), "name".to_owned()),
            },
            ComposeProjection {
                name: "last_message_preview".to_owned(),
                source: ProjectionSource::Subselect("last_message_preview".to_owned()),
            },
            ComposeProjection {
                name: "unread_count".to_owned(),
                source: ProjectionSource::Subselect("unread_count".to_owned()),
            },
        ],
        subselects: vec![
            // latest body of ChatMessage related_by chat_message.chat order created_at desc
            ComposeSubselect {
                name: "last_message_preview".to_owned(),
                kind: SubselectKind::Latest {
                    column: "body".to_owned(),
                    resource: TypeRef::UserDefined(qname("ChatMessage")),
                },
                related_by: FkPath::from_segments(["chat_message", "chat"]),
                where_pred: Vec::new(),
                filter_pred: Vec::new(),
                order: vec![OrderBy {
                    field: "created_at".to_owned(),
                    direction: OrderDir::Desc,
                }],
            },
            // count ChatMessage related_by chat_message.chat
            //   where read_at = nil AND sender != ctx.user.id
            ComposeSubselect {
                name: "unread_count".to_owned(),
                kind: SubselectKind::Count(TypeRef::UserDefined(qname("ChatMessage"))),
                related_by: FkPath::from_segments(["chat_message", "chat"]),
                where_pred: vec![Predicate::And(vec![
                    Predicate::Comparison {
                        left: Expr::Path(Path::from_segments(["self", "read_at"])),
                        op: CompareOp::Eq,
                        right: Expr::Nil,
                    },
                    Predicate::Comparison {
                        left: Expr::Path(Path::from_segments(["self", "sender"])),
                        op: CompareOp::Ne,
                        right: Expr::Path(Path::from_segments(["ctx", "user", "id"])),
                    },
                ])],
                filter_pred: Vec::new(),
                order: Vec::new(),
            },
        ],
        filters: Vec::new(),
        key: None,
        scope: Vec::new(),
        scope_override: false,
        order: vec![OrderBy {
            field: "created_at".to_owned(),
            direction: OrderDir::Desc,
        }],
        paginate: Some(50),
        policy: PolicyRef::Local("read".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        returns: TypeRef::UserDefined(qname("ChatInboxRow")),
        scope_origin: ComposeScopeOrigin::Inherited,
        owner_scope_sql: None,
        previous_names: Vec::new(),
        span_ref: None,
    }));
    feature
}

#[test]
fn compose_chat_inbox_emits_joins_subselects_and_tenant_predicate() {
    let feature = chat_inbox_feature();
    let out = emit(&feature).expect("compose must emit a query.gen.go");

    // --- the query value lowers onto the SQL-backed (QueryView) path -------
    assert!(
        out.contains("var chatInbox = lazuli.Query[ChatInboxArgs, ChatInboxRow]{"),
        "compose query value missing:\n{out}"
    );
    assert!(
        out.contains("Kind:     lazuli.QueryView,"),
        "compose must reuse the SQL-backed QueryView runtime path:\n{out}"
    );
    assert!(
        out.contains("SQLText: \""),
        "compose must embed the generated SELECT inline (no opaque engine):\n{out}"
    );

    // --- the JOIN (FK-path lowered to a real ON clause) --------------------
    assert!(
        out.contains("LEFT JOIN \\\"user\\\" \\\"cp\\\" ON \\\"cp\\\".\\\"id\\\" = \\\"root\\\".\\\"counterpart\\\""),
        "optional FK join must lower to a LEFT JOIN with a generated ON clause:\n{out}"
    );

    // --- the correlated subselects -----------------------------------------
    // unread COUNT, correlated on the chat FK, with the closed `where` predicate.
    assert!(
        out.contains("(SELECT COUNT(*) FROM \\\"chat_message\\\" c WHERE c.\\\"chat\\\" = root.\\\"id\\\""),
        "unread COUNT subselect must be a correlated scalar sub-select:\n{out}"
    );
    assert!(
        out.contains("AS \\\"unread_count\\\""),
        "unread COUNT must alias to its projection name:\n{out}"
    );
    // last-message preview: latest body ORDER BY created_at DESC LIMIT 1.
    assert!(
        out.contains("(SELECT c.\\\"body\\\" FROM \\\"chat_message\\\" c WHERE c.\\\"chat\\\" = root.\\\"id\\\" ORDER BY \\\"created_at\\\" DESC LIMIT 1)"),
        "latest subselect must be ORDER BY ... LIMIT 1:\n{out}"
    );

    // --- THE MOAT: generated tenant + soft-delete predicate in root WHERE --
    // Columns match the runtime `baseScopeConditions` convention (unquoted
    // `deleted_at`/`org_id`). The tenant bind is `$2` because the unread
    // subselect's actor ref claimed `$1` first (binds follow SQL text order).
    assert!(
        out.contains("WHERE root.deleted_at IS NULL AND root.org_id = $2"),
        "the GENERATED tenant + soft-delete predicate must be in the root WHERE \
         (scope inheritance reaching the emitted SQL is the verifiability proof):\n{out}"
    );

    // --- the unread `where` actor predicate is BOUND from ctx, not inlined --
    // `sender != ctx.user.id` → `c.sender != $N`, and the bind resolves via
    // lazuli.CtxValue in SQLArgsCtx (so a model can't drop the actor clause).
    assert!(
        out.contains("c.\\\"read_at\\\" IS NULL"),
        "read_at = nil must lower to IS NULL:\n{out}"
    );
    assert!(
        out.contains("c.\\\"sender\\\" != $1"),
        "the actor predicate must allocate a positional bind ($1, before tenant):\n{out}"
    );
    assert!(
        out.contains("SQLArgsCtx: func(ctx *lazuli.Ctx, args ChatInboxArgs) []any {"),
        "compose must emit the ctx-aware bind projection:\n{out}"
    );
    assert!(
        out.contains("lazuli.CtxValue(ctx, \"user.id\"),"),
        "the actor bind must resolve from ctx:\n{out}"
    );
    assert!(
        out.contains("lazuli.CtxValue(ctx, \"tenant.org_id\"),"),
        "the tenant bind must resolve from ctx (not a param):\n{out}"
    );

    // --- list semantics: SQLMany true + ORDER BY + LIMIT -------------------
    assert!(out.contains("SQLMany: true,"), "no `key` ⇒ many rows:\n{out}");
    assert!(
        out.contains("ORDER BY \\\"created_at\\\" DESC\\nLIMIT 50"),
        "list compose must carry ORDER BY + paginate LIMIT:\n{out}"
    );

    // --- generated return record struct, db tags == SQL aliases ------------
    // Struct rows are column-aligned, so assert on the (name, type) pair and
    // the `db`/`json` tag independently of inter-column padding.
    assert!(
        out.contains("type ChatInboxRow struct {"),
        "generated return record struct missing:\n{out}"
    );
    assert!(
        field_row(&out, "ChatID", "lazuli.ID", "chat_id"),
        "self.id projection must type as lazuli.ID with its alias db tag:\n{out}"
    );
    assert!(
        field_row(&out, "UnreadCount", "int64", "unread_count"),
        "count subselect must type as int64:\n{out}"
    );
    assert!(
        field_row(&out, "LastMessagePreview", "*string", "last_message_preview"),
        "latest subselect must type as a nullable *string:\n{out}"
    );
    assert!(
        field_row(&out, "CounterpartName", "*string", "counterpart_name"),
        "LEFT-joined counterpart name must be nullable:\n{out}"
    );

    // --- exported Go wrapper delegates to the shared RunSQL path ------------
    assert!(
        out.contains("func ChatInbox(ctx *lazuli.Ctx, args ChatInboxArgs) ([]ChatInboxRow, error) {"),
        "list compose wrapper signature missing:\n{out}"
    );
    assert!(
        out.contains("return out.([]ChatInboxRow), nil"),
        "wrapper must run through RunSQL and assert the row slice:\n{out}"
    );
}

/// Single-row form: a `key` clause flips the compose to `SQLMany: false`
/// (the runtime returns `not_found` on zero rows, §3.2 #6) and the wrapper
/// returns a single row. Models `catalog.property_kpis` (key self.id =
/// params.property_id) with an aggregate sub-select.
#[test]
fn compose_single_row_key_emits_sqlmany_false_and_aggregate() {
    let mut feature = base_feature("catalog");
    feature.resources.push(resource("Property", Vec::new()));
    feature.resources.push(resource(
        "ServiceTransaction",
        vec![
            field("property", TypeRef::UserDefined(qname("Property")), true),
            field(
                "total_amount_cents",
                TypeRef::Builtin(BuiltinType::Integer),
                true,
            ),
            field("status", TypeRef::Builtin(BuiltinType::Text), true),
        ],
    ));

    feature.queries.push(Query::Compose(ComposeQuery {
        name: "property_kpis".to_owned(),
        public_contract: None,
        root: TypeRef::UserDefined(qname("Property")),
        params: vec![lazuli_ir::TypedSlot {
            name: "property_id".to_owned(),
            type_ref: TypeRef::Builtin(BuiltinType::Id),
            required: true,
            constraints: lazuli_ir::FieldConstraints::default(),
            validate_skip: false,
        }],
        joins: Vec::new(),
        projections: vec![
            ComposeProjection {
                name: "property_id".to_owned(),
                source: ProjectionSource::SelfCol("id".to_owned()),
            },
            ComposeProjection {
                name: "revenue_cents".to_owned(),
                source: ProjectionSource::Subselect("revenue_cents".to_owned()),
            },
        ],
        subselects: vec![ComposeSubselect {
            name: "revenue_cents".to_owned(),
            kind: SubselectKind::Aggregate {
                func: AggFn::Sum,
                column: "total_amount_cents".to_owned(),
                resource: TypeRef::UserDefined(qname("ServiceTransaction")),
            },
            related_by: FkPath::from_segments(["service_transaction", "property"]),
            where_pred: Vec::new(),
            filter_pred: vec![Predicate::Comparison {
                left: Expr::Path(Path::from_segments(["self", "status"])),
                op: CompareOp::Eq,
                right: Expr::String("paid".to_owned()),
            }],
            order: Vec::new(),
        }],
        filters: Vec::new(),
        key: Some(KeyClause {
            path: Path::from_segments(["self", "id"]),
            equals: Expr::Path(Path::from_segments(["params", "property_id"])),
        }),
        scope: Vec::new(),
        scope_override: false,
        order: Vec::new(),
        paginate: None,
        policy: PolicyRef::None,
        policy_expr: None,
        policy_when_denied: None,
        returns: TypeRef::UserDefined(qname("PropertyKpiRow")),
        scope_origin: ComposeScopeOrigin::Inherited,
        owner_scope_sql: None,
        previous_names: Vec::new(),
        span_ref: None,
    }));

    let out = emit(&feature).expect("compose must emit");
    assert!(out.contains("SQLMany: false,"), "key ⇒ single row:\n{out}");
    // Aggregate SUM with a FILTER (WHERE ...) clause from `filter`.
    assert!(
        out.contains("(SELECT SUM(c.\\\"total_amount_cents\\\") FILTER (WHERE c.\\\"status\\\" = 'paid') FROM \\\"service_transaction\\\" c WHERE c.\\\"chat\\\" = root.\\\"id\\\"")
            || out.contains("(SELECT SUM(c.\\\"total_amount_cents\\\") FILTER (WHERE c.\\\"status\\\" = 'paid') FROM \\\"service_transaction\\\" c WHERE c.\\\"property\\\" = root.\\\"id\\\""),
        "aggregate must render SUM(...) FILTER (WHERE ...) correlated on the related FK:\n{out}"
    );
    // The key predicate binds params.property_id.
    assert!(
        out.contains("root.\\\"id\\\" = $1"),
        "the key clause must bind the identity predicate:\n{out}"
    );
    assert!(
        out.contains("args.PropertyID,"),
        "the param bind must read from the typed args struct:\n{out}"
    );
    // Single-row wrapper returns one row.
    assert!(
        out.contains("func PropertyKpis(ctx *lazuli.Ctx, args PropertyKpisArgs) (PropertyKpiRow, error) {"),
        "single-row wrapper signature missing:\n{out}"
    );
}

/// `scope override` suppresses the generated tenant predicate (§5.2): the
/// author's policy/scope governs, so codegen emits NO `org_id` in the root
/// WHERE and emits the breadcrumb.
#[test]
fn compose_scope_override_suppresses_tenant_predicate() {
    let mut feature = base_feature("messaging");
    let mut chat = resource("Chat", Vec::new());
    chat.tenancy = Some(Tenancy::Org);
    chat.soft_delete = true;
    feature.resources.push(chat);

    feature.queries.push(Query::Compose(ComposeQuery {
        name: "all_chats".to_owned(),
        public_contract: None,
        root: TypeRef::UserDefined(qname("Chat")),
        params: Vec::new(),
        joins: Vec::new(),
        projections: vec![ComposeProjection {
            name: "chat_id".to_owned(),
            source: ProjectionSource::SelfCol("id".to_owned()),
        }],
        subselects: Vec::new(),
        filters: Vec::new(),
        key: None,
        scope: Vec::new(),
        scope_override: true,
        order: Vec::new(),
        paginate: None,
        policy: PolicyRef::Local("admin".to_owned()),
        policy_expr: None,
        policy_when_denied: None,
        returns: TypeRef::UserDefined(qname("AllChatsRow")),
        scope_origin: ComposeScopeOrigin::Overridden,
        owner_scope_sql: None,
        previous_names: Vec::new(),
        span_ref: None,
    }));

    let out = emit(&feature).expect("compose must emit");
    assert!(
        !out.contains("org_id = $1"),
        "scope override must suppress the generated tenant predicate:\n{out}"
    );
    assert!(
        out.contains("// scope override: inherited tenant/soft-delete predicate suppressed"),
        "scope override must emit a traceability breadcrumb:\n{out}"
    );
    // With no binds at all the projection returns nil.
    assert!(
        out.contains("return nil"),
        "a compose with no binds must emit `return nil` in SQLArgsCtx:\n{out}"
    );
}
