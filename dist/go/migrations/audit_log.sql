CREATE TABLE IF NOT EXISTS audit_log (
    id BIGSERIAL PRIMARY KEY,
    org_id BIGINT,                          -- nullable for system actors
    actor_id BIGINT,                        -- nullable for anonymous
    actor_kind TEXT NOT NULL,               -- 'user' | 'system' | 'service'
    command_name TEXT NOT NULL,             -- "customer.create"
    target_resource TEXT,                   -- "Customer"
    target_id BIGINT,                       -- nullable
    payload JSONB,                          -- request input
    result_status TEXT NOT NULL,            -- 'ok' | 'error'
    error_code TEXT,                        -- when result_status='error'
    happened_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    correlation_id TEXT
);

CREATE INDEX audit_log_command_idx ON audit_log (command_name);
CREATE INDEX audit_log_target_idx ON audit_log (target_resource, target_id);
CREATE INDEX audit_log_actor_idx ON audit_log (actor_id);
CREATE INDEX audit_log_org_time_idx ON audit_log (org_id, happened_at DESC);
