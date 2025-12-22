-- Initial AIngle Wasm schema

CREATE TABLE IF NOT EXISTS Wasm (
    hash            BLOB           PRIMARY KEY ON CONFLICT IGNORE,
    blob            BLOB           NOT NULL
);

CREATE TABLE IF NOT EXISTS SafDef (
    hash            BLOB           PRIMARY KEY ON CONFLICT IGNORE,
    blob            BLOB           NOT NULL
);

CREATE TABLE IF NOT EXISTS EntryDef (
    key             BLOB           PRIMARY KEY ON CONFLICT IGNORE,
    blob            BLOB           NOT NULL
);