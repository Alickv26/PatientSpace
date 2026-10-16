-- Clinicians table — stores login credentials for facility staff.
CREATE TABLE IF NOT EXISTS clinicians (
    id          TEXT    PRIMARY KEY,          -- UUID
    username    TEXT    NOT NULL UNIQUE,
    full_name   TEXT    NOT NULL,
    role        TEXT    NOT NULL DEFAULT 'nurse',  -- clerk, nurse, doctor, pharmacist, lab_tech, admin, auditor
    password_hash TEXT NOT NULL,              -- argon2 hash
    created_at  INTEGER NOT NULL,
    active      INTEGER NOT NULL DEFAULT 1
);

-- Sessions table — HttpOnly cookie tokens.
CREATE TABLE IF NOT EXISTS sessions (
    token       TEXT    PRIMARY KEY,          -- random 32-byte hex
    clinician_id TEXT   NOT NULL,
    created_at  INTEGER NOT NULL,
    expires_at  INTEGER NOT NULL,
    FOREIGN KEY (clinician_id) REFERENCES clinicians(id)
);

CREATE INDEX IF NOT EXISTS idx_sessions_clinician ON sessions(clinician_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires ON sessions(expires_at);
