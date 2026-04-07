CREATE TABLE privacy_pass_token (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    token BLOB NOT NULL
);

CREATE TABLE batched_token_key (
    token_key_id INTEGER PRIMARY KEY,
    public_key BLOB NOT NULL
);
