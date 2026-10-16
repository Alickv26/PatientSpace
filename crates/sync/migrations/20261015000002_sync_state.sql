CREATE TABLE IF NOT EXISTS sync_state (
    facility_id  TEXT NOT NULL,
    key          TEXT NOT NULL,
    value        BLOB NOT NULL,
    PRIMARY KEY (facility_id, key)
);
