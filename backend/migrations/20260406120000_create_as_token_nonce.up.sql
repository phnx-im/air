CREATE TYPE nonce_status AS ENUM ('reserved', 'committed');

CREATE UNLOGGED TABLE as_token_nonce (
    nonce BYTEA PRIMARY KEY,
    status nonce_status NOT NULL DEFAULT 'reserved',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
