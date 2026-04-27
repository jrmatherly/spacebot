-- Postgres port of migrations/global/20260407120000_wiki.sql.
-- The biggest divergence: SQLite FTS5 virtual table → Postgres tsvector + GIN.
-- The application layer dispatches search() per-variant (Pattern C in the
-- PR 11.2 plan): SQLite calls `... MATCH ?` against wiki_pages_fts; Postgres
-- calls `... search_tsv @@ websearch_to_tsquery('english', $1)` directly
-- against wiki_pages.

CREATE TABLE IF NOT EXISTS wiki_pages (
    -- gen_random_uuid() is built-in since pg13. We hex-encode without dashes
    -- to match SQLite's lower(hex(randomblob(16))) format byte-for-byte so
    -- IDs are interchangeable between backends.
    id TEXT PRIMARY KEY DEFAULT replace(gen_random_uuid()::text, '-', ''),
    slug TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    page_type TEXT NOT NULL CHECK (
        page_type IN (
            'entity',
            'concept',
            'decision',
            'project',
            'reference'
        )
    ),
    content TEXT NOT NULL DEFAULT '',
    related TEXT NOT NULL DEFAULT '[]',
    created_by TEXT NOT NULL,
    updated_by TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    archived INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    updated_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),

    -- Postgres-only: tsvector backing FTS. STORED generated column keeps it
    -- in sync without trigger machinery and lets the GIN index serve the
    -- websearch_to_tsquery operator natively.
    search_tsv tsvector GENERATED ALWAYS AS (
        setweight(to_tsvector('english', coalesce(title, '')), 'A') ||
        setweight(to_tsvector('english', coalesce(content, '')), 'B')
    ) STORED
);

CREATE TABLE IF NOT EXISTS wiki_page_versions (
    id TEXT PRIMARY KEY DEFAULT replace(gen_random_uuid()::text, '-', ''),
    page_id TEXT NOT NULL REFERENCES wiki_pages (id),
    version INTEGER NOT NULL,
    content TEXT NOT NULL,
    edit_summary TEXT,
    author_type TEXT NOT NULL CHECK (
        author_type IN ('agent', 'user')
    ),
    author_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT to_char(now() AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),
    UNIQUE (page_id, version)
);

-- Indexes (parity with SQLite naming)
CREATE INDEX IF NOT EXISTS wiki_pages_type ON wiki_pages (page_type);
CREATE INDEX IF NOT EXISTS wiki_pages_updated ON wiki_pages (updated_at DESC);
CREATE INDEX IF NOT EXISTS wiki_pages_archived ON wiki_pages (archived);
CREATE INDEX IF NOT EXISTS wiki_versions_page ON wiki_page_versions (page_id, version DESC);

-- GIN index on the tsvector backs FTS queries.
CREATE INDEX IF NOT EXISTS wiki_pages_search_tsv ON wiki_pages USING GIN (search_tsv);
