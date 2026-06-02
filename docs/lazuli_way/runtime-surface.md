<!-- GENERATED FILE — DO NOT EDIT BY HAND.
     Source of truth: runtime/go/lazuli/**/*.go (the exported Go surface).
     Regenerate with: cargo run -p xtask -- gen-runtime-surface
     Freshness is gated by tools/xtask/tests/runtime_surface_fresh.rs. -->

# Lazuli runtime surface — what the runtime already owns

The runtime owns the MECHANISM; you declare the RESOURCE. Before you write a
handler, scan this index: if the runtime already exports the verb, delegate —
do NOT reimplement. See docs/lazuli_way/delegate-to-runtime.md.

This index is generated from `runtime/go/lazuli/` and is **intent-keyed**: the
families the pilot audit flagged for reinvention (auth, lifecycle, CRUD, roles,
money, semantic scalars, …) carry a *"reach for this when you need to X"* line.
A flat list of symbols does not defeat reinvention; an intent-keyed one does.

<!-- BEGIN GENERATED BODY -->

## Reach for this (intent-keyed)

The families the pilot audit flagged for reinvention. Reach for the runtime verb; do not hand-roll it.

### Password hashing
**Reach for this when you need to:** hash or verify a user password.

`auth.HashPassword` · `auth.VerifyPassword` · `auth.HashWithArgon2` · `auth.Argon2Params` · `auth.SetArgon2Concurrency`

> argon2id + concurrency cap + tuning are runtime-owned. Prefer dropping `@fn.hash_password` and letting `@cap.Hashed(algorithm:argon2id)` auto-wire. **Never import `golang.org/x/crypto/argon2`.**

### Sessions
**Reach for this when you need to:** mint, hash, issue, rotate, or invalidate a session.

`auth.MintSessionToken` · `auth.HashSessionToken` · `auth.IssueSession` · `auth.RotateSession` · `auth.ResolveSession` · `auth.InvalidateSession` · `auth.InvalidateSessionByID`

> token + hash stay in lockstep with the runtime session resolver. Don't hand-roll opaque-token mint/hash with `crypto/rand` + `crypto/sha256`.

### Password reset / email verification
**Reach for this when you need to:** issue or consume a reset/verify token.

`auth.RequestPasswordReset` · `auth.ConfirmPasswordReset` · `auth.PasswordResetToken` · `auth.PasswordResetContract` · `auth.IssueEmailVerificationToken` · `auth.VerifyEmailToken` · `auth.EmailVerificationToken` · `auth.EmailVerificationContract`

> format + hashing + TTL + single-use consume come from the runtime against the declared contract. No manual `SELECT … used_at … expiry` dance.

### JWT / OAuth / MFA
**Reach for this when you need to:** sign/verify a JWT, run an OAuth leg, enroll/verify MFA.

`auth.SignJWT` · `auth.VerifyJWT` · `auth.Claims` · `auth.BuildOAuthConfig` · `auth.GoogleOAuthRedirectURL` · `auth.GoogleOAuthCallback` · `auth.EnrollMFA` · `auth.VerifyMFA`

> the auth leg is wired; you supply the binding, not the crypto.

### Roles
**Reach for this when you need to:** check the actor's role.

`lazuli.HasRole` · `lazuli.RequireRole` · `lazuli.RequireActor`

> inline `lazuli.HasRole(ctx,"ADMIN")` — never an app-local `actorHasRole` helper.

### Money
**Reach for this when you need to:** construct or parse a money value.

`lazuli.BRL` · `lazuli.USD` · `lazuli.EUR` · `lazuli.MoneyValue` · `lazuli.ParseMoneyLiteral`

> currency-aware money is a runtime type; see `docs/lazuli_way/money.md`.

### Rate limit
**Reach for this when you need to:** parse / apply a rate-limit spec.

`lazuli.ParseRateLimit` · `lazuli.RateLimit` · `lazuli.RateLimitFromDefault` · `lazuli.RateLimitByEnv` · `lazuli.RateLimitMiddleware`

> declare `rate_limit "<spec>"`; the runtime parses + enforces.

### Lifecycle / state transitions
**Reach for this when you need to:** advance a resource's status through a state machine.

`lazuli.TransitionAdvance` · `lazuli.Transition` · `lazuli.LifecycleStateMismatchError`

> declare a `lifecycle <field>` + `transition`; the runtime enforces the from-state set and emits the mismatch error. Never hand-roll `UPDATE … status IN (…) … RowsAffected == 0`. The generic FSM lives in package `lifecycle` (`lifecycle.Machine` / `lifecycle.New` / `lifecycle.Transition`). See `docs/lazuli_way/state-machines.md`.

### CRUD effects
**Reach for this when you need to:** create / update / delete / reorder a resource.

`lazuli.Creates` · `lazuli.CreatesEffect` · `lazuli.CreatesWithOwnerCheck` · `lazuli.Updates` · `lazuli.UpdatesEffect` · `lazuli.Deletes` · `lazuli.DeletesEffect` · `lazuli.Reorder` · `lazuli.ReorderEffect` · `lazuli.NewUpdate` · `lazuli.PartialUpdate` · `lazuli.UpdateBuilder` · `lazuli.OwnedByActor`

> declare `creates`/`updates`/`deletes`; the runtime applies defaults, `ctx.now`, actor-owner scoping. No raw `db.Exec` INSERT with literal defaults; no hand-built partial-UPDATE placeholder bookkeeping. See `docs/lazuli_way/crud-by-convention.md`.

### Semantic scalars
**Reach for this when you need to:** validate a hex color / percentage / positive amount / non-negative count / email at the boundary.

`lazuli.HexColor` · `lazuli.Percentage` · `lazuli.PositiveDecimal` · `lazuli.NonNegativeInt`

> declare the field as the scalar (`color: @semantic.HexColor`, `price: @semantic.PositiveDecimal`, `stock: @semantic.NonNegativeInt`) from the closed `@semantic.*` catalog (`@semantic.HexColor`, `@semantic.Percentage`, `@semantic.PositiveDecimal`, `@semantic.NonNegativeInt`, `@semantic.Email`, `@semantic.Money`); the type validates at decode. Never a hand-written `> 0` / `>= 0` validator or a regex `^#?[0-9A-Fa-f]{6}$`.

### Notifications
**Reach for this when you need to:** send a notification / digest.

`notifications.Send` · `notifications.NewRegistry` · `notifications.NotificationContract` · `notifications.NewSMTPDispatcher`

> the channel/digest/throttle machinery is runtime-owned.

### Storage / signed URLs
**Reach for this when you need to:** issue a signed upload/download URL.

`storage.IssueSignedURL` · `storage.IssueSignedUploadURL` · `storage.NewS3Store` · `storage.ObjectStore`

> declare `@cap.File`; the runtime owns presigning.

### Encryption
**Reach for this when you need to:** encrypt/decrypt a field at rest.

`encryption.NewCipher` · `encryption.For` · `encryption.ForCtx` · `encryption.Register`

> declare `@cap.Encrypted(key:@key.*)`; the cipher + key rotation are runtime-owned (maps geocoding is `lazuli.Geocoder`).

## Full symbol census

_Generated from 719 exported symbols across 31 runtime packages._

### audit
> audit-trail sinks + rows.

`NoopSink` · `Row` · `Sink`

### auth
> password/session/reset/JWT/OAuth/MFA — the auth mechanism (see intent section).

`Argon2Params` · `AuditEntry` · `AuditFromCtx` · `BuildOAuthConfig` · `Claims` · `ConfirmPasswordReset` · `EmailVerificationContract` · `EmailVerificationToken` · `EmitAudit` · `EnrollMFA` · `FieldRef` · `GenerateOAuthState` · `GoogleOAuthCallback` · `GoogleOAuthRedirectURL` · `GoogleUserInfo` · `HashPassword` · `HashSessionToken` · `HashWithArgon2` · `InvalidateSession` · `InvalidateSessionByID` · `IssueEmailVerificationToken` · `IssueSession` · `LoadOAuthState` · `LoginHandler` · `LogoutHandler` · `MapSessionResolveError` · `MfaContract` · `MfaEnrolment` · `MfaMethod` · `MintSessionToken` · `OAuthCallback` · `OAuthContract` · `OAuthRedirect` · `PasswordAlgorithm` · `PasswordContract` · `PasswordResetContract` · `PasswordResetToken` · `PopulateExpiredSignal` · `RefreshHandler` · `RefreshInput` · `RefreshOutput` · `RegisterRefreshContract` · `RegisterSessionContract` · `RequestPasswordReset` · `ResolveAccessWithExpiry` · `ResolveSession` · `ResolvedSession` · `RotateSession` · `SessionAttrs` · `SessionDB` · `SessionsContract` · `SetArgon2Concurrency` · `SignJWT` · `SignupHandler` · `StashOAuthState` · `TheftAction` · `VerifyEmailToken` · `VerifyJWT` · `VerifyMFA` · `VerifyPassword`

### billing
> plan catalog, quotas, feature gates, subscriptions.

`ActiveSubscription` · `CheckFeature` · `CheckQuota` · `ErrPlanFeatureForbidden` · `ErrPlanLookupFailed` · `ErrPlanQuotaExceeded` · `FeatureSet` · `GateKind` · `GateRef` · `IncrQuota` · `LimitInt` · `LimitValue` · `LookupPlan` · `NewFeatureSet` · `PeriodStart` · `Plan` · `PlanCatalog` · `Register` · `RegisterCatalog` · `RegisterUsage` · `Resolve` · `SubscriptionAnchor` · `SubscriptionStore` · `TrialPolicy` · `UsageStore`

### breach
> breached-credential checking.

`Checker` · `NoopChecker`

### cache
> query cache: tags, invalidation, Redis backend.

`Active` · `Backend` · `Bind` · `IntersectTags` · `InvalidationTarget` · `MustGet` · `NewRedisBackend` · `Of` · `Query` · `QuerySpec` · `QueryStats` · `QueryTarget` · `QueryWildcard` · `QueryWildcardTarget` · `RedisBackend` · `SpecBuilder` · `Tag` · `TagTarget`

### captcha
> captcha verification.

`NoopVerifier` · `Verdict` · `Verifier`

### email
> SMTP / Sendgrid mail adapters.

`SMTPAdapter` · `SendgridAdapter`

### encryption
> field-at-rest cipher + key rotation (see intent section).

`Algorithm` · `Binding` · `Bindings` · `Cipher` · `CtxAxes` · `For` · `ForCtx` · `FormatID` · `NewCipher` · `Register` · `RegisterCtxAxes` · `Reset` · `Rotation` · `Source` · `TemplateAxis` · `TenantIDFromContext` · `UserIDFromContext`

### events
> event dispatch, outbox/inbox dedup pump.

`Dispatcher` · `InboxDedup` · `NewInboxDedup` · `OutboxRow` · `PumpOnce` · `StartPump`

### i18n
> locale negotiation, catalogs, error-message resolution.

`AppErrorResolverRegistry` · `BuiltinCatalog` · `BuiltinLocales` · `Catalog` · `Default` · `DefaultResolver` · `ErrorExposureDefault` · `ErrorKeys` · `ErrorRequest` · `ErrorResolver` · `Fallback` · `FeatureErrorContract` · `LocaleContract` · `LocaleFrom` · `MessageRef` · `Middleware` · `NegotiateAcceptLanguage` · `NewDefaultResolver` · `RegisterFeatureTranslationCatalog` · `SetDefaultResolver` · `SetEscapeRenderer` · `ShouldExpose` · `WithLocale`

### jobs
> background jobs: dispatch, retries, idempotency (River).

`BackoffStrategy` · `DispatchJob` · `Dispatcher` · `ExternalCallRef` · `FanoutSpec` · `HandlerFunc` · `IdempotencyKeySpec` · `Idempotent` · `JobContract` · `JobEnvelope` · `JobTrigger` · `LazuliJobArgs` · `NewRiverDispatcher` · `NextDelay` · `RegisterIncrementRunner` · `RegisterJobs` · `RegisterPreludeRunner` · `Retry` · `RetryBuilder` · `RetryPolicy` · `RiverDispatcher` · `RiverInserter` · `ShouldRetry` · `TenantFromSpec` · `WithTimeout`

### lazuli
> the core runtime package: ctx, db, effects, policy, roles, money, rate-limit, lifecycle, semantic scalars (see intent section).

`ActivityRow` · `Actor` · `Adapter` · `AdapterError` · `AdapterRef` · `Allow` · `Api` · `AppCors` · `AppErrorResolverRegistry` · `AppIntegrationName` · `AppLocaleContract` · `AppRoute` · `Approval` · `ApprovalBuilder` · `ApprovalSpec` · `ApprovalThen` · `AuditSpec` · `AuthGuard` · `AutoPhotoClear` · `AutoPhotoConfirm` · `AutoPhotoConfirmArgs` · `AutoPhotoDisplayURL` · `AutoPhotoGetURL` · `AutoPhotoRequest` · `AutoPhotoRequestArgs` · `AutoPhotoSpec` · `AutoPhotoUploadIntent` · `BRL` · `BindingFn` · `Bindings` · `Boot` · `BuildInfo` · `BuildInfoData` · `CSPBuilder` · `CSRFMiddleware` · `CacheSpec` · `CacheStats` · `Chain` · `ClassifyDBError` · `Command` · `Commands` · `ConfigureSessionCookie` · `ConstantManifest` · `Context` · `CookieOpts` · `CorsMiddleware` · `Creates` · `CreatesEffect` · `CreatesWithOwnerCheck` · `Ctx` · `DB` · `DBTX` · `Date` · `DefaultCSPBuilder` · `DefaultTenantResolverFn` · `DeleteCookie` · `Deletes` · `DeletesEffect` · `Deprecation` · `Duration` · `EUR` · `Effect` · `EnvConfig` · `EnvironmentFromContext` · `Error` · `ErrorBase` · `ErrorBaseFromContext` · `ErrorExposureDefault` · `ErrorKeys` · `EvalPolicy` · `EvalPolicyInput` · `Event` · `EventDeriveFrom` · `EventDescriptor` · `EventEmit` · `EventGroup` · `EventTraceEmit` · `ExternalCallRef` · `FeatureErrorContract` · `Field` · `FieldError` · `FieldReason` · `FilterRule` · `FlushCache` · `FromConst` · `FromCtx` · `FromCtxOwnedVia` · `FromFn` · `FromInput` · `FromInputOptional` · `FromTarget` · `GateKind` · `GateRef` · `Geocoder` · `GetCookie` · `HandlerFromRegistry` · `HasMany` · `HasRole` · `HexColor` · `HttpMethod` · `ID` · `IdempotencyKey` · `Index` · `InstallConstantManifest` · `LibBugError` · `LifecycleStateMismatchError` · `LoadEnv` · `LockAdapterRegistry` · `LookupCommand` · `LookupKey` · `LookupQuery` · `LookupResource` · `LookupValidator` · `Middleware` · `MiddlewareFunc` · `MoneyValue` · `MustNewPool` · `MustResolveAdapter` · `Mux` · `NewCSRFGuard` · `NewPool` · `NewUpdate` · `NonNegativeInt` · `NotFoundError` · `NotOwnerError` · `ObjectStore` · `ObservabilityErrorSourcesFromContext` · `OrderClause` · `OutboxMode` · `OwnedByActor` · `OwnerCheckSpec` · `PanicRecoverFromContext` · `ParseMoneyLiteral` · `ParseRateLimit` · `PartialUpdate` · `Percentage` · `Policy` · `PolicyAtom` · `PolicyError` · `PolicyWhen` · `PositiveDecimal` · `Publish` · `Queries` · `Query` · `QueryKind` · `RateLimit` · `RateLimitByEnv` · `RateLimitFromDefault` · `RateLimitKey` · `RateLimitMiddleware` · `RateLimitSpec` · `RbacPermissionChecker` · `RbacRoleChecker` · `RecordActivity` · `RecoverMiddleware` · `Register` · `RegisterAdapter` · `RegisterApi` · `RegisterAppErrorResolver` · `RegisterAppIntegration` · `RegisterAppLocaleContract` · `RegisterBindingFn` · `RegisterCommand` · `RegisterErrorRendererEscape` · `RegisterFeatureErrors` · `RegisterFeatureTranslationCatalog` · `RegisterFn` · `RegisterQuery` · `RegisterRbac` · `RegisterResource` · `RegisterSessionResolver` · `RegisterUniqueViolationCode` · `RegisterValidator` · `Registerable` · `Registry` · `Reorder` · `ReorderEffect` · `RequestID` · `RequestIDMiddleware` · `RequireActor` · `RequireRole` · `ResolveAdapter` · `ResolveAppIntegration` · `ResolveConstant` · `ResolveSecret` · `ResolveTyped` · `Resource` · `Resources` · `Retention` · `RetentionAction` · `RetentionSpec` · `RetryPolicy` · `Returns` · `ReturnsEffect` · `ReturnsFromRegistry` · `RouteKind` · `RunIncrement` · `RunPrelude` · `RunRetentionScan` · `SanitizeHTML` · `SanitizeHTMLProfile` · `ScheduleRuleDate` · `SearchMode` · `SearchSpec` · `SessionCookieConfig` · `SessionCookieName` · `SessionExpiryResolver` · `SessionResolver` · `SessionRolesResolver` · `SetCSPBuilder` · `SetCookie` · `SetCorsContract` · `SetDB` · `SetEnvironment` · `SetIfNotNilEnum` · `SetIfNotNilSlice` · `SetObservabilityPolicy` · `SetPanicRecoverPolicy` · `SetSessionCookieSecure` · `SetStore` · `SetTrustedProxies` · `Source` · `SourceTag` · `SourceTagFromContext` · `StartRetentionWorker` · `Stats` · `Store` · `StringPtr` · `Subscribe` · `Subscriber` · `SubscribersOf` · `Surface` · `TenancyMode` · `Tenant` · `TenantError` · `TenantOrgID` · `Time` · `Transition` · `TransitionAdvance` · `TypedDecode` · `USD` · `UpdateBuilder` · `Updates` · `UpdatesEffect` · `User` · `V` · `Validatable` · `ValidateApiHandlers` · `ValidateValue` · `ValidatorFunc` · `ValidatorRef` · `WaitForShutdown` · `WithDefaultTenant` · `WithSource`

### lifecycle
> generic state-machine FSM (`Machine`/`New`/`Transition`).

`Machine` · `New` · `Transition`

### maps
> geocoding request/response types.

`GeoPoint` · `GeocodeRequest` · `GeocodeResponse` · `Geocoder` · `ReverseGeocodeRequest` · `ReverseGeocodeResponse`

### mcp
> Model-Context-Protocol server/client/tool/resource registration.

`AuthSpec` · `Client` · `ClientEndpoint` · `ClientFailMode` · `ClientImport` · `ClientImportKind` · `ClientRegistration` · `Dial` · `HTTPHandler` · `PromptHandler` · `PromptHandlerFromRegistry` · `PromptMessage` · `PromptRegistration` · `RegisterPrompt` · `RegisterResource` · `RegisterServer` · `RegisterTool` · `RegisteredServers` · `ResourceHandler` · `ResourceHandlerFromRegistry` · `ResourceRegistration` · `Serve` · `ServerMetadata` · `ServerRegistration` · `ToolHandler` · `ToolHandlerFromRegistry` · `ToolRegistration` · `Transport`

### migrations
> tenant-aware migration planning + deploy strategy.

`BackoffStrategy` · `Checkpoint` · `DeployPolicy` · `DeployStrategy` · `Dispatcher` · `HandlerFunc` · `IdempotencyKeySpec` · `Logger` · `MigrationBuilder` · `MigrationContract` · `NewDispatcher` · `On` · `Planner` · `PlannerOutcome` · `RetryPolicy` · `Target` · `TenantDirectory` · `TenantMigrationContract` · `TenantMigrationTarget` · `TenantMigrator`

### mtls
> mutual-TLS identity verification.

`DenyVerifier` · `Identity` · `Verifier`

### notifications
> channels, digests, throttles, SMTP dispatch (see intent section).

`Channel` · `ChannelDispatcher` · `DigestKey` · `DigestStore` · `DigestStrategy` · `Envelope` · `IdempotencyKeySpec` · `MemoryDigestStore` · `MemoryThrottleStore` · `NewMemoryDigestStore` · `NewMemoryThrottleStore` · `NewRegistry` · `NewSMTPDispatcher` · `NotificationContract` · `NotificationDigest` · `NotificationThrottle` · `Registry` · `RetryPolicy` · `SMTPConfig` · `SMTPConfigFromEnv` · `SMTPDispatcher` · `Send` · `TenantFromSpec` · `ThrottleKey` · `ThrottleStore`

### observability
> logging, tracing, audit records, health/readiness probes.

`Active` · `AgentRunPayload` · `AuditDestination` · `AuditRecord` · `BuildAuditRecord` · `CommandRunPayload` · `Configure` · `ConfigureTracing` · `DurationMS` · `EmitAgentRun` · `EmitAudit` · `EmitCommandRun` · `EmitJobRun` · `EmitTraceEvent` · `EmitWebhookRun` · `HealthHandler` · `HealthProbeSet` · `JobRunPayload` · `LivenessHandler` · `LogFormat` · `LogLevel` · `LoggerContract` · `LoggingContract` · `MarkStart` · `NewJSONHandler` · `NewLogger` · `NewTracer` · `PanicError` · `PanicScope` · `ReadinessHandlerFn` · `ReadinessProbe` · `RecoverHTTP` · `RecoverScope` · `RedactStrategy` · `Redactor` · `RegexRedactor` · `RegisterCheck` · `RegisterDBCheck` · `RegisterProbes` · `SetRedactor` · `SetTraceEventSink` · `SpanContract` · `StartOp` · `StartSpan` · `ToolCall` · `Trace` · `TraceEvent` · `TracerContract` · `TracingContract` · `WebhookRunPayload`

### payments
> payment gateway + webhook event types.

`PaymentGateway` · `Preference` · `PreferenceRequest` · `WebhookEvent` · `WebhookStatus`

### plangate
> plan-gate kinds + refs.

`GateKind` · `GateRef`

### poller
> external-status polling: schedulers, predicates, backoff.

`AttemptsAccessor` · `Backoff` · `Bind` · `Cursor` · `EvalPredicate` · `EventPublisher` · `Exponential` · `Fixed` · `GenderFlipOnce` · `Linear` · `NewRegistry` · `NewScheduler` · `PendingResult` · `QueryRunner` · `Quirk` · `Register` · `Registry` · `ResolveFunc` · `ResolveResult` · `Retry` · `RowDecoder` · `Scheduler` · `Spec` · `SpecHandle` · `StateKind` · `State_` · `TerminalResult` · `Tick`

### probe
> liveness / readiness probe constructors.

`Liveness` · `NewLiveness` · `NewReadiness` · `Readiness`

### report
> CSV/XLSX report runners, columns, params, policy.

`CSVWriter` · `Column` · `ColumnSource` · `Contract` · `ErrRunnerNotWired` · `FnCall` · `Format` · `FormatFromToken` · `Input` · `MissingInputError` · `Mount` · `MustPattern` · `NewCSVWriter` · `NewXLSXWriter` · `Params` · `ParamsFromContext` · `ParseInputs` · `Pattern` · `PolicyAtom` · `PolicyChecker` · `RateLimiter` · `Register` · `Registered` · `Registration` · `Reset` · `RowField` · `Run` · `Runner` · `SetPolicyChecker` · `SetRateLimiter` · `SetRunner` · `SourceFn` · `WithParams` · `XLSXOptions` · `XLSXWriter`

### reputation
> reputation scoring.

`NeutralScorer` · `Score` · `Scorer`

### runtime
> codegen-emitted glue sentinels + constructors (referential-guard `ErrReferencedInUse` / `NewReferencedInUseError`).

`NewReferencedInUseError` · `ReferencedInUseError`

### secrets
> secret providers + leased secrets.

`EnvProvider` · `LeasedSecret` · `Provider`

### storage
> object store + signed URLs (see intent section).

`App` · `AuthCheck` · `FetchPrivate` · `FileContract` · `FileRef` · `FileVisibility` · `ImageAny` · `ImageMime` · `IssueSignedURL` · `IssueSignedURLAt` · `IssueSignedUploadURL` · `Key` · `LocalStore` · `Metadata` · `MimeType` · `MinIOStore` · `NewLocalStore` · `NewMinIOStore` · `NewS3Store` · `ObjectMeta` · `ObjectStore` · `PresignedURLWriter` · `Private` · `Provider` · `Public` · `S3Store` · `Signed` · `TextMime` · `Upload`

### vectorstore
> vector store: collections, embeddings, similarity queries.

`Collection` · `Embedder` · `Item` · `Match` · `VectorFilter` · `VectorQuery` · `VectorStore`

### waf
> web-application-firewall filters + decisions.

`Decision` · `Filter` · `NoopFilter` · `Reason`

### webhooks
> inbound/outbound webhooks: routing, HMAC verify, replay, DLQ.

`DlqKind` · `DlqSpec` · `EmitBinding` · `EmitPredicateKind` · `Envelope` · `EventPublisher` · `HandlerFunc` · `Mount` · `Register` · `RegisterEventPublisher` · `RegisterIdempotencyChecker` · `RegisterIncrementRunner` · `RegisterPreludeRunner` · `Registered` · `ReplayMode` · `ReplaySpec` · `ResetRegistryForTesting` · `Router` · `TenantFromSpec` · `VerifyHmacSignature` · `VerifyScheme` · `VerifySpec` · `WebhookContract` · `WebhookEventRef`
