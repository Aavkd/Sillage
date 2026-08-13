-- Sillage — initial schema (phase 02).
--
-- The database is the *index*: `data/<id>.json` holds the complete record (CONCEPTION.md §3.4)
-- and this file holds what has to be queried — the library list, the tags, the full-text search,
-- the queue. Everything here can be rebuilt from the JSON files.
--
-- Every path stored is **relative to the library root**, so moving the library folder
-- (ROADMAP phase 02, task 6) does not invalidate a single row.

CREATE TABLE transcripts (
    id              TEXT    PRIMARY KEY,
    -- Where the file came from, as the user gave it. Absolute, and only ever displayed.
    source_path     TEXT    NOT NULL,
    -- The library's own copy: `media/<id>.<ext>`, relative to the library root.
    media_path      TEXT    NOT NULL,
    sha256          TEXT    NOT NULL,
    created_at      INTEGER NOT NULL,          -- epoch milliseconds
    duration_ms     INTEGER NOT NULL,
    title           TEXT    NOT NULL,
    title_is_custom INTEGER NOT NULL DEFAULT 0,
    language        TEXT,                      -- NULL until whisper has decided
    model           TEXT    NOT NULL,
    status          TEXT    NOT NULL,          -- queued | running | done | failed
    error           TEXT,                      -- French, actionable, when status = 'failed'
    -- SHA-256 of the displayed text. Compared with llm_outputs.source_transcript_hash to raise
    -- the OBSOLÈTE badge of phase 08.
    transcript_hash TEXT    NOT NULL DEFAULT '',
    -- Denormalised copies, kept only to feed the full-text index below. `body` is the displayed
    -- text (verbatim + corrections); `summary` mirrors the LLM summary and is maintained by
    -- trigger.
    body            TEXT    NOT NULL DEFAULT '',
    summary         TEXT    NOT NULL DEFAULT ''
);

-- The library lists by date, newest first (CONCEPTION.md §5.1).
CREATE INDEX transcripts_created_at ON transcripts (created_at DESC);
-- Duplicate detection (CONCEPTION.md §8). Not unique: the user may deliberately transcribe the
-- same file twice, and the second entry must be allowed to exist.
CREATE INDEX transcripts_sha256 ON transcripts (sha256);

CREATE TABLE segments (
    transcript_id TEXT    NOT NULL REFERENCES transcripts (id) ON DELETE CASCADE,
    idx           INTEGER NOT NULL,            -- position in the transcript
    -- Derived from the first and last word, never taken from whisper's own segment timestamps:
    -- those are wrong after a silence, by up to 16,6 s (spike/RESULTS.md §4.2).
    start_ms      INTEGER NOT NULL,
    end_ms        INTEGER NOT NULL,
    text          TEXT    NOT NULL,
    PRIMARY KEY (transcript_id, idx)
);

CREATE TABLE words (
    transcript_id TEXT    NOT NULL REFERENCES transcripts (id) ON DELETE CASCADE,
    -- Stable for the whole life of the transcript: the corrections layer of phase 07 is anchored
    -- to it (CONCEPTION.md §3.5).
    word_id       INTEGER NOT NULL,
    segment_idx   INTEGER NOT NULL,
    idx           INTEGER NOT NULL,            -- position inside the segment
    start_ms      INTEGER NOT NULL,
    end_ms        INTEGER NOT NULL,
    text          TEXT    NOT NULL,
    prob          REAL    NOT NULL,            -- drives the low-confidence marking, DESIGN.md §8
    PRIMARY KEY (transcript_id, word_id)
);

CREATE INDEX words_position ON words (transcript_id, segment_idx, idx);
-- « cliquer un mot → lire à cet endroit » works the other way round too: find the word playing
-- at a given instant.
CREATE INDEX words_time ON words (transcript_id, start_ms);

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE transcript_tags (
    transcript_id TEXT    NOT NULL REFERENCES transcripts (id) ON DELETE CASCADE,
    tag_id        INTEGER NOT NULL REFERENCES tags (id) ON DELETE CASCADE,
    PRIMARY KEY (transcript_id, tag_id)
);

CREATE INDEX transcript_tags_tag ON transcript_tags (tag_id);

CREATE TABLE llm_outputs (
    id                     TEXT    PRIMARY KEY,
    transcript_id          TEXT    NOT NULL REFERENCES transcripts (id) ON DELETE CASCADE,
    -- cleaned | summary | notes | custom:<slug>  (CONCEPTION.md §3.4)
    kind                   TEXT    NOT NULL,
    provider               TEXT    NOT NULL,
    model                  TEXT    NOT NULL,
    prompt_version         INTEGER NOT NULL,
    -- The transcript_hash the content was generated from. Different from the current one ⇒
    -- the output is stale (CONCEPTION.md §3.4, decision #18).
    source_transcript_hash TEXT    NOT NULL,
    generated_at           INTEGER NOT NULL,
    content                TEXT    NOT NULL
);

-- One output of each kind per transcript; regenerating replaces it.
CREATE UNIQUE INDEX llm_outputs_kind ON llm_outputs (transcript_id, kind);

CREATE TABLE queue_items (
    id            TEXT    PRIMARY KEY,
    transcript_id TEXT    NOT NULL REFERENCES transcripts (id) ON DELETE CASCADE,
    -- Rank in the single, strictly sequential queue (CONCEPTION.md §3.3).
    position      INTEGER NOT NULL,
    state         TEXT    NOT NULL,            -- queued | running | done | failed
    enqueued_at   INTEGER NOT NULL,
    started_at    INTEGER,
    error         TEXT
);

-- A transcript sits in the queue at most once.
CREATE UNIQUE INDEX queue_items_transcript ON queue_items (transcript_id);
CREATE INDEX queue_items_position ON queue_items (position);

-- Library-local state: anything that must travel with the library folder rather than with the
-- machine. The user's own settings live in the app config directory instead — the location of
-- this very folder is one of them, so it cannot be stored inside it (phase 01 decision).
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Full-text search over title, body and summary (ROADMAP phase 02, task 3).
--
-- `remove_diacritics 2` is the point of the exercise in French: it folds diacritics on both the
-- indexed text and the query, so « résumé » finds « resume » and the other way round, and no
-- French word is ever unreachable because of how it was typed.
CREATE VIRTUAL TABLE transcripts_fts USING fts5 (
    title,
    body,
    summary,
    content = 'transcripts',
    content_rowid = 'rowid',
    tokenize = "unicode61 remove_diacritics 2"
);

CREATE TRIGGER transcripts_fts_ai AFTER INSERT ON transcripts BEGIN
    INSERT INTO transcripts_fts (rowid, title, body, summary)
    VALUES (new.rowid, new.title, new.body, new.summary);
END;

CREATE TRIGGER transcripts_fts_ad AFTER DELETE ON transcripts BEGIN
    INSERT INTO transcripts_fts (transcripts_fts, rowid, title, body, summary)
    VALUES ('delete', old.rowid, old.title, old.body, old.summary);
END;

-- Restricted to the three indexed columns on purpose: a status or a progress update must not
-- reindex the text of a two-hour transcript.
CREATE TRIGGER transcripts_fts_au AFTER UPDATE OF title, body, summary ON transcripts BEGIN
    INSERT INTO transcripts_fts (transcripts_fts, rowid, title, body, summary)
    VALUES ('delete', old.rowid, old.title, old.body, old.summary);
    INSERT INTO transcripts_fts (rowid, title, body, summary)
    VALUES (new.rowid, new.title, new.body, new.summary);
END;

-- The summary lives in llm_outputs; these three keep the indexed copy in step with it, and the
-- update above then carries the change into the index.
CREATE TRIGGER llm_outputs_summary_ai AFTER INSERT ON llm_outputs
WHEN new.kind = 'summary' BEGIN
    UPDATE transcripts SET summary = new.content WHERE id = new.transcript_id;
END;

CREATE TRIGGER llm_outputs_summary_au AFTER UPDATE ON llm_outputs
WHEN new.kind = 'summary' BEGIN
    UPDATE transcripts SET summary = new.content WHERE id = new.transcript_id;
END;

CREATE TRIGGER llm_outputs_summary_ad AFTER DELETE ON llm_outputs
WHEN old.kind = 'summary' BEGIN
    UPDATE transcripts SET summary = '' WHERE id = old.transcript_id;
END;
