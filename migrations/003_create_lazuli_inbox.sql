-- EVENT-OUTBOX §3.3 — receiver-side idempotency table.
--
-- Subscribers call `InboxDedup.AlreadySeen(envelope_id)` before
-- invoking the user handler and `InboxDedup.MarkSeen(envelope_id, ...)`
-- after success. The primary-key conflict on `envelope_id` makes
-- replays from a crashed outbox pump idempotent at the database.

CREATE TABLE IF NOT EXISTS lazuli_inbox (
    envelope_id    TEXT         PRIMARY KEY,
    event_name     TEXT         NOT NULL,
    seen_at        TIMESTAMPTZ  NOT NULL DEFAULT now()
);
