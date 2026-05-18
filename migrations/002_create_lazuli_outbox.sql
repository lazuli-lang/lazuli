-- EVENT-OUTBOX §3.3 — transactional outbox table.
--
-- Producers (generated command handlers) INSERT one row per
-- `outbox guaranteed` event inside the same pgx tx as the resource
-- mutation. The outbox pump in runtime/lazuli/events selects
-- undispatched rows, dispatches them, then marks `dispatched_at`.

CREATE TABLE IF NOT EXISTS lazuli_outbox (
    id             BIGSERIAL    PRIMARY KEY,
    envelope_id    TEXT         NOT NULL,
    event_name     TEXT         NOT NULL,
    payload        JSONB        NOT NULL,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT now(),
    dispatched_at  TIMESTAMPTZ
);

-- Partial index keeps the pump's `WHERE dispatched_at IS NULL` scan
-- bounded as the outbox accumulates dispatched history.
CREATE INDEX IF NOT EXISTS lazuli_outbox_undispatched_idx
    ON lazuli_outbox (dispatched_at)
    WHERE dispatched_at IS NULL;
