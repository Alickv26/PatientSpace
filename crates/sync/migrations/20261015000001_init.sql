CREATE TABLE IF NOT EXISTS envelopes (
    facility_id         TEXT    NOT NULL,
    version_id          BLOB    NOT NULL,
    resource_id         BLOB    NOT NULL,
    resource_type       TEXT    NOT NULL,
    envelope_blob       BLOB    NOT NULL,
    created_at_unix_ms  INTEGER NOT NULL,
    ingested_at_unix_ms INTEGER NOT NULL,
    PRIMARY KEY (version_id)
);

CREATE INDEX IF NOT EXISTS idx_envelopes_resource
    ON envelopes(resource_id, created_at_unix_ms DESC);

CREATE INDEX IF NOT EXISTS idx_envelopes_facility_time
    ON envelopes(facility_id, ingested_at_unix_ms);

CREATE INDEX IF NOT EXISTS idx_envelopes_type
    ON envelopes(resource_type, created_at_unix_ms DESC);

CREATE TABLE IF NOT EXISTS vector_clocks (
    resource_id  BLOB    NOT NULL,
    facility_id  TEXT    NOT NULL,
    counter      INTEGER NOT NULL,
    PRIMARY KEY (resource_id, facility_id)
);

CREATE TABLE IF NOT EXISTS causal_edges (
    version_id         BLOB NOT NULL,
    parent_version_id  BLOB NOT NULL,
    PRIMARY KEY (version_id, parent_version_id)
);

CREATE INDEX IF NOT EXISTS idx_causal_edges_parent
    ON causal_edges(parent_version_id);
