//! Feature-emit tests for jobs. Split out of `mod.rs` to keep
//! production under the 500 LOC budget.

    use super::*;
    use lazuli_ir::{
        AppManifest, Assignment, BackoffStrategy, CommandEffect, CreateEffect, Defaults, Expr,
        ExternalCallRef, FanoutScope, IdempotencyKey, JobDeclarative, JobHandler, JobTrigger,
        LetBinding, Module, NamedArg, Path, Policies, PolicyRef, QualifiedName, Resource,
        RetryPolicy, TenantFromSpec, TypeRef, UpdateEffect,
    };

    fn base_feature(name: &str) -> Feature {
        Feature {
            name: name.to_owned(),
            purpose: None,
            non_goals: Vec::new(),
            context_path: None,
            defaults: Defaults {
                tenancy: None,
                timestamps: false,
                policy: None,
            },
            uses: Vec::new(),
            uses_spans: Vec::new(),
            uses_versions: Vec::new(),
            requirements: Vec::new(),
            enums: Vec::new(),
            resources: Vec::new(),
            events: Vec::new(),
            rules: Vec::new(),
            policies: Policies {
                categories: Vec::new(),
                fields: Vec::new(),
                span_ref: None,
            },
            errors: None,
            commands: Vec::new(),
            apis: Vec::new(),
            records: Vec::new(),
            queries: Vec::new(),
            resume_routers: Vec::new(),
            workflows: Vec::new(),
            jobs: Vec::new(),
            webhooks: Vec::new(),
            notifications: Vec::new(),
            event_groups: Vec::new(),
            tenant_migrations: Vec::new(),
            translation: None,
            pollers: vec![],
            auth: None,
            surfaces: Vec::new(),
            extensions: Vec::new(),
            escape_routes: Vec::new(),
            agents: Vec::new(),
            reports: Vec::new(),
            channels: Vec::new(),
            caches: Vec::new(),
            aggregates: vec![],
            mcp_servers: vec![],
            previous_names: Vec::new(),
            span_ref: None,
            synth_origins: std::collections::BTreeMap::new(),
        }
    }

    fn minimal_app() -> AppManifest {
        AppManifest {
            name: "test".to_owned(),
            title: None,
            version: None,
            lazuli_version: None,
            targets: Vec::new(),
            default_locale: None,
            default_timezone: None,
            auth_failed_redirect: None,
            not_found: None,
            error_pages: Vec::new(),
            uses: Vec::new(),
            packs: Vec::new(),
            bindings: Vec::new(),
            architecture: None,
            services: Vec::new(),
            communication: None,
            environments: Vec::new(),
            urls: Vec::new(),
            cors: None,
            headers: None,
            cookie: None,
            proxy: None,
            limits: None,
            env: Vec::new(),
            integrations: Vec::new(),
            capabilities: Vec::new(),
            runtime: Vec::new(),
            deploy: None,
            logging: None,
            tracing: None,
            observability: None,
            locale: None,
            encryption_bindings: Vec::new(),
            route_guard: None,
            actor_query: None,
            span_ref: None,
        }
    }

    fn module_with_feature(feature: Feature) -> Module {
        Module {
            workspace: None,
            contracts: Vec::new(),
            app: Some(minimal_app()),
            registry: None,
            profiles: Vec::new(),
            design: None,
            rbac: None,
            features: vec![feature],
        }
    }

    fn emit(feature: &Feature) -> Option<String> {
        let module = module_with_feature(feature.clone());
        let index = CrossFeatureIndex::build(&module);
        let emit_ctx = EmitContext::no_source("customer/job.gen.go");
        emit_job_file("examples/x.lzi", feature, "lazuli/test", &index, &emit_ctx)
    }

    fn event_qname(feature: &str, name: &str) -> QualifiedName {
        QualifiedName {
            feature: Some(feature.to_owned()),
            name: name.to_owned(),
        }
    }

    fn local_qname(name: &str) -> QualifiedName {
        QualifiedName {
            feature: None,
            name: name.to_owned(),
        }
    }

    fn handler_job(name: &str) -> Job {
        Job {
            name: name.to_owned(),
            trigger: JobTrigger::Event {
                event: event_qname("customer", "customer_created"),
            },
            queue: None,
            idempotency: None,
            retry: None,
            policy: None,
            policy_expr: None,
            policy_when_denied: None,
            tenant_from: None,
            fanout: None,
            timeout: None,
            external_calls: Vec::new(),
            body: JobBody::Handler(JobHandler {
                path: lazuli_ir::PathRef::authored(format!("./jobs/{name}.go")),
                returns: None,
            }),
            emits: Vec::new(),
            previous_names: Vec::new(),
            span_ref: None,
        }
    }

    #[test]
    fn event_handler_job_emits_full_runtime_contract() {
        let mut feature = base_feature("customer_import");
        let mut job = handler_job("process_import");
        job.trigger = JobTrigger::Event {
            event: event_qname("customer_import", "customer_import_uploaded"),
        };
        job.queue = Some("customer_imports".to_owned());
        job.tenant_from = Some(TenantFromSpec {
            path: Path::from_segments(["payload", "org_id"]),
        });
        job.idempotency = Some(IdempotencyKey {
            by: Path::from_segments(["payload", "batch_id"]),
        });
        job.retry = Some(RetryPolicy {
            count: 3,
            backoff: BackoffStrategy::Exponential,
        });
        job.timeout = Some("30s".to_owned());
        job.external_calls = vec![ExternalCallRef {
            slot: "crm".to_owned(),
            op: "normalize_import_batch".to_owned(),
            args: vec![
                NamedArg {
                    name: "org_id".to_owned(),
                    value: Expr::Path(Path::from_segments(["payload", "org_id"])),
                },
                NamedArg {
                    name: "batch_id".to_owned(),
                    value: Expr::Path(Path::from_segments(["payload", "batch_id"])),
                },
            ],
            span_ref: None,
        }];
        job.emits = vec!["customer_import_completed".to_owned()];
        feature.jobs.push(job);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("\"time\""));
        assert!(out.contains("\"lazuli.dev/runtime/lazuli/jobs\""));
        assert!(out.contains("var customerImportProcessImportJob = jobs.JobContract{"));
        assert!(out.contains("Trigger:     jobs.JobTrigger{Kind: \"event\", Event: \"customer_import.customer_import_uploaded\"},"));
        assert!(out.contains("Queue:       \"customer_imports\","));
        assert!(out.contains("Timeout:     30 * time.Second,"));
        assert!(out.contains("HandlerPath: \"./jobs/process_import.go\","));
        assert!(out.contains("Path: \"payload.org_id\","));
        assert!(out.contains("Path: \"payload.batch_id\","));
        assert!(out.contains("Backoff: jobs.BackoffExponential,"));
        assert!(out.contains("{Slot: \"crm\", Operation: \"normalize_import_batch\", Args: []string{\"batch_id=payload.batch_id\", \"org_id=payload.org_id\"}},"));
        assert!(out.contains("Emits: []string{\"customer_import_completed\"},"));
    }

    #[test]
    fn scheduled_job_emits_fanout_retry_and_default_policy() {
        let mut feature = base_feature("customer");
        feature.defaults.policy = Some(PolicyRef::Atom("actor.system".to_owned()));
        let mut job = handler_job("recompute_scores");
        job.trigger = JobTrigger::Schedule {
            cron: "0 2 * * *".to_owned(),
        };
        job.fanout = Some(lazuli_ir::FanoutSpec {
            scope: FanoutScope::Tenants,
            axis: "org".to_owned(),
        });
        job.idempotency = Some(IdempotencyKey {
            by: Path::from_segments(["tenant", "org_id"]),
        });
        job.retry = Some(RetryPolicy {
            count: 3,
            backoff: BackoffStrategy::Exponential,
        });
        feature.jobs.push(job);

        let out = emit(&feature).expect("must emit");
        assert!(
            out.contains("Trigger:     jobs.JobTrigger{Kind: \"schedule\", Cron: \"0 2 * * *\"},")
        );
        assert!(out.contains("Policy:      \"@actor.system\","));
        assert!(out.contains("Fanout: &jobs.FanoutSpec{"));
        assert!(out.contains("Scope: \"tenants\","));
        assert!(out.contains("Axis:  \"org\","));
    }

    #[test]
    fn declarative_body_surfaces_runtime_gap_comments() {
        let mut feature = base_feature("customer");
        feature.resources.push(Resource {
            name: "Customer".to_owned(),
            public_contract: None,
            tenancy: None,
            soft_delete: false,
            timestamps: None,
            fields: Vec::new(),
            constraints: Vec::new(),
            validate: None,
            validates: Vec::new(),
            retention: None,
            previous_names: Vec::new(),
            span_ref: None,
            lifecycle: None,
            invariants: vec![],

            lock: None,

            composite_key: None,
            conventions: Vec::new(),
            lifecycle_routes: None,
        });
        let mut job = handler_job("recompute_score_after_invoice");
        job.body = JobBody::Declarative(JobDeclarative {
            target: Some(lazuli_ir::TargetExpr {
                query: local_qname("by_id"),
                args: Vec::new(),
            }),
            lets: vec![LetBinding {
                name: "new_score".to_owned(),
                value: Expr::Path(Path::from_segments(["fn", "risk_score"])),
            }],
            effect: CommandEffect::Updates(UpdateEffect {
                resource: local_qname("Customer"),
                assignments: vec![Assignment {
                    field: "score".to_owned(),
                    value: Expr::Path(Path::from_segments(["new_score"])),
                }],
            }),
        });
        feature.jobs.push(job);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains(
            "// TODO(runtime): declarative job bodies are not represented in jobs.JobContract yet."
        ));
        assert!(out.contains("// TODO(runtime): preserve declarative target by_id."));
        assert!(out.contains("// TODO(runtime): preserve declarative let bindings new_score."));
        assert!(out.contains("// TODO(runtime): preserve declarative effect updates Customer."));
        assert!(!out.contains("HandlerPath:"));
    }

    #[test]
    fn handler_returns_and_unparsed_timeout_emit_runtime_todos() {
        let mut feature = base_feature("billing");
        let mut job = handler_job("export");
        job.timeout = Some("5 minutes".to_owned());
        job.body = JobBody::Handler(JobHandler {
            path: lazuli_ir::PathRef::authored("./jobs/export.go"),
            returns: Some(TypeRef::UserDefined(local_qname("InvoiceExport"))),
        });
        feature.jobs.push(job);

        let out = emit(&feature).expect("must emit");
        assert!(out.contains("HandlerPath: \"./jobs/export.go\","));
        assert!(out.contains("// TODO(runtime): JobContract.Timeout is time.Duration; cannot preserve authored duration \"5 minutes\" without a parser helper."));
        assert!(out.contains("// TODO(runtime): handler returns InvoiceExport, but jobs.JobContract has no return type field."));
        assert!(!out.contains("\"time\""));
    }

    #[test]
    fn deterministic_across_runs_and_sorts_jobs_by_name() {
        let mut feature = base_feature("customer");
        feature.jobs.push(handler_job("zebra"));
        feature.jobs.push(handler_job("alpha"));

        let a = emit(&feature).expect("must emit");
        let b = emit(&feature).expect("must emit");
        assert_eq!(a, b);

        let alpha_pos = a.find("Job: customer.alpha").expect("alpha banner");
        let zebra_pos = a.find("Job: customer.zebra").expect("zebra banner");
        assert!(alpha_pos < zebra_pos);
    }

    #[test]
    fn feature_emit_entry_point_emits_job_file_shape() {
        let mut feature = base_feature("warehouse");
        let mut job = handler_job("sync_inventory");
        job.trigger = JobTrigger::Schedule {
            cron: "*/15 * * * *".to_owned(),
        };
        job.queue = Some("inventory".to_owned());
        feature.jobs.push(job);

        let out = emit(&feature).expect("emit_job_file must emit for a feature with jobs");

        assert!(!out.is_empty());
        assert!(out.contains("// Code generated by lazuli; DO NOT EDIT."));
        assert!(out.contains("package warehousegen"));
        assert!(out.contains("var warehouseSyncInventoryJob = jobs.JobContract{"));
        assert!(
            out.contains(
                "Trigger:     jobs.JobTrigger{Kind: \"schedule\", Cron: \"*/15 * * * *\"},"
            )
        );
        assert!(out.contains("Queue:       \"inventory\","));
    }

    #[allow(dead_code)]
    fn _create_effect_compiles(_: CreateEffect) {}

    #[test]
    fn gated_job_emits_real_prelude_field_and_billing_imports() {
        // PG.C.2 — gated jobs lift the wave-4 comment annotation
        // into a real `Prelude: []billing.GateRef{...}` field that
        // `jobs.DispatchJob` (and the River worker wrapper) consult
        // via the package-level runner billing registers at init.
        let mut feature = base_feature("billing");
        let job = handler_job("dispatch_payment");
        feature.jobs.push(job);

        let mut gates: std::collections::BTreeMap<String, Vec<lazuli_ir::Gate>> =
            std::collections::BTreeMap::new();
        gates.insert(
            "billing/job:dispatch_payment".to_owned(),
            vec![
                lazuli_ir::Gate::Behind {
                    feature: "dispatch_payment".to_owned(),
                },
                lazuli_ir::Gate::Quota {
                    limit: "payments_per_month".to_owned(),
                },
            ],
        );
        let module = module_with_feature(feature);
        let index = CrossFeatureIndex::build(&module);
        let emit_ctx =
            EmitContext::for_feature(None, "billing-app", "billing", "billing/job.gen.go")
                .with_gates(Some(&gates));
        let out = emit_job_file(
            "examples/billing.lzi",
            &module.features[0],
            "billing-app",
            &index,
            &emit_ctx,
        )
        .expect("must emit");

        assert!(
            out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "billing import missing:\n{out}"
        );
        assert!(
            out.contains("\"billing-app/plan\""),
            "plan import missing:\n{out}"
        );
        assert!(
            out.contains("Prelude: []billing.GateRef{"),
            "Prelude field missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateBehind, Name: \"dispatch_payment\"},"),
            "behind-gate row missing:\n{out}"
        );
        assert!(
            out.contains("{Kind: billing.GateQuota, Name: \"payments_per_month\"},"),
            "quota-gate row missing:\n{out}"
        );
    }

    #[test]
    fn ungated_job_emits_no_prelude_or_billing_import() {
        let mut feature = base_feature("customer");
        feature.jobs.push(handler_job("ping"));
        let out = emit(&feature).expect("must emit");
        assert!(!out.contains("Prelude:"), "no Prelude when no gates");
        assert!(
            !out.contains("\"lazuli.dev/runtime/lazuli/billing\""),
            "no billing import when no gates"
        );
    }
