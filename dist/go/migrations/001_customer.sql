-- Hand-written migration for the runtime spike. In the eventual generated
-- world this comes from `lazuli generate`, which derives DDL from resource
-- declarations.

CREATE TABLE customer (
    id          BIGSERIAL PRIMARY KEY,
    org_id      BIGINT      NOT NULL,
    name        TEXT        NOT NULL,
    email       TEXT        NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at  TIMESTAMPTZ
);

CREATE INDEX customer_org_id_idx ON customer (org_id);
CREATE UNIQUE INDEX customer_email_org_id_uniq ON customer (org_id, email)
    WHERE deleted_at IS NULL;
