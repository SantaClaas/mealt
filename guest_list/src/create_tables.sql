BEGIN;

CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email_address TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    expires_at_utc INTEGER NOT NULL,
    FOREIGN KEY(user_id) REFERENCES users(id)
);

CREATE TABLE IF NOT EXISTS signin_attempts (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    expires_at_utc INTEGER NOT NULL,
    FOREIGN KEY(user_id) REFERENCES users(id)
);

END;
