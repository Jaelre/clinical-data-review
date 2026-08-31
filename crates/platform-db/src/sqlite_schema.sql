PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS platform_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS tenants (
    id BLOB PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    settings TEXT,
    metadata TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS users (
    id BLOB PRIMARY KEY,
    email TEXT UNIQUE,
    local_identifier TEXT UNIQUE,
    first_name TEXT,
    last_name TEXT,
    display_name TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS user_tenant_roles (
    id BLOB PRIMARY KEY,
    user_id BLOB NOT NULL,
    tenant_id BLOB NOT NULL,
    role TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(user_id, tenant_id, role),
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS local_work_sessions (
    id BLOB PRIMARY KEY,
    tenant_id BLOB NOT NULL,
    operator_id BLOB NOT NULL,
    session_label TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_activity_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    ended_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY(operator_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS patients (
    id BLOB PRIMARY KEY,
    external_id TEXT NOT NULL,
    age INTEGER,
    sex TEXT,
    tenant_id BLOB NOT NULL,
    review_status TEXT NOT NULL DEFAULT 'pending',
    priority_level INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(external_id, tenant_id),
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS patient_notes (
    id BLOB PRIMARY KEY,
    patient_id BLOB NOT NULL,
    tenant_id BLOB NOT NULL,
    category TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(patient_id, category),
    FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE,
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS clinical_journal (
    id BLOB PRIMARY KEY,
    patient_id BLOB NOT NULL,
    tenant_id BLOB NOT NULL,
    entry_timestamp TEXT NOT NULL,
    entry_sequence INTEGER NOT NULL,
    role TEXT,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(patient_id, entry_sequence),
    FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE,
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS research_sessions (
    id BLOB PRIMARY KEY,
    tenant_id BLOB NOT NULL,
    session_name TEXT NOT NULL,
    primary_researcher_id BLOB,
    status TEXT NOT NULL DEFAULT 'active',
    total_patients INTEGER NOT NULL DEFAULT 0,
    patients_per_chunk INTEGER NOT NULL DEFAULT 0,
    current_chunk_number INTEGER NOT NULL DEFAULT 1,
    current_chunk_patients TEXT NOT NULL DEFAULT '[]',
    completed_chunks TEXT NOT NULL DEFAULT '[]',
    cohort_id BLOB,
    started_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    completed_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_activity_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY(primary_researcher_id) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY(cohort_id) REFERENCES research_cohorts(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS judgments (
    id BLOB PRIMARY KEY,
    patient_id BLOB NOT NULL UNIQUE,
    tenant_id BLOB NOT NULL,
    reviewer_id BLOB,
    local_work_session_id BLOB,
    research_session_id BLOB,
    judgment TEXT NOT NULL,
    judgment_notes TEXT,
    judgment_made_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE,
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY(reviewer_id) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY(local_work_session_id) REFERENCES local_work_sessions(id) ON DELETE SET NULL,
    FOREIGN KEY(research_session_id) REFERENCES research_sessions(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS admin_flags (
    id BLOB PRIMARY KEY,
    patient_id BLOB NOT NULL,
    tenant_id BLOB NOT NULL,
    created_by BLOB,
    flag_type TEXT NOT NULL,
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    resolution_notes TEXT,
    resolved_by BLOB,
    resolved_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(patient_id, flag_type),
    FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE,
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY(created_by) REFERENCES users(id) ON DELETE SET NULL,
    FOREIGN KEY(resolved_by) REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS cohort_ingestion (
    id TEXT PRIMARY KEY,
    cohort_name TEXT NOT NULL,
    patient_external_id TEXT NOT NULL,
    tenant_slug TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    error_message TEXT,
    display_order INTEGER,
    ingestion_metadata TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    processed_at TEXT
);

CREATE TABLE IF NOT EXISTS research_cohorts (
    id BLOB PRIMARY KEY,
    tenant_id BLOB NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    external_cohort_id TEXT,
    total_patients INTEGER NOT NULL DEFAULT 0,
    cohort_type TEXT NOT NULL DEFAULT 'imported',
    selection_criteria TEXT,
    created_by BLOB,
    research_protocol TEXT,
    study_metadata TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    archived_at TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    metadata TEXT,
    UNIQUE(tenant_id, name),
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
    FOREIGN KEY(created_by) REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS research_cohort_patients (
    cohort_id BLOB NOT NULL,
    patient_id BLOB NOT NULL,
    display_order INTEGER NOT NULL DEFAULT 0,
    inclusion_reason TEXT,
    patient_metadata TEXT,
    added_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    added_by BLOB,
    PRIMARY KEY(cohort_id, patient_id),
    FOREIGN KEY(cohort_id) REFERENCES research_cohorts(id) ON DELETE CASCADE,
    FOREIGN KEY(patient_id) REFERENCES patients(id) ON DELETE CASCADE,
    FOREIGN KEY(added_by) REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS research_cohort_reviewers (
    cohort_id BLOB NOT NULL,
    user_id BLOB NOT NULL,
    role TEXT NOT NULL,
    can_review INTEGER NOT NULL DEFAULT 1,
    can_export INTEGER NOT NULL DEFAULT 0,
    can_modify_cohort INTEGER NOT NULL DEFAULT 0,
    granted_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    granted_by BLOB,
    expires_at TEXT,
    access_metadata TEXT,
    PRIMARY KEY(cohort_id, user_id),
    FOREIGN KEY(cohort_id) REFERENCES research_cohorts(id) ON DELETE CASCADE,
    FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE,
    FOREIGN KEY(granted_by) REFERENCES users(id) ON DELETE SET NULL
);

CREATE TABLE IF NOT EXISTS research_cohort_batches (
    id BLOB PRIMARY KEY,
    cohort_id BLOB NOT NULL,
    tenant_id BLOB NOT NULL,
    batch_number INTEGER NOT NULL,
    patient_external_ids TEXT NOT NULL DEFAULT '[]',
    is_empty INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE(cohort_id, batch_number),
    FOREIGN KEY(cohort_id) REFERENCES research_cohorts(id) ON DELETE CASCADE,
    FOREIGN KEY(tenant_id) REFERENCES tenants(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_local_identifier ON users(local_identifier);
CREATE INDEX IF NOT EXISTS idx_roles_user_tenant ON user_tenant_roles(user_id, tenant_id);
CREATE INDEX IF NOT EXISTS idx_patients_tenant ON patients(tenant_id);
CREATE INDEX IF NOT EXISTS idx_patients_external_id ON patients(tenant_id, external_id);
CREATE INDEX IF NOT EXISTS idx_judgments_patient ON judgments(patient_id);
CREATE INDEX IF NOT EXISTS idx_judgments_reviewer ON judgments(tenant_id, reviewer_id);
CREATE INDEX IF NOT EXISTS idx_admin_flags_patient ON admin_flags(tenant_id, patient_id, status);
CREATE INDEX IF NOT EXISTS idx_local_work_sessions_active ON local_work_sessions(tenant_id, operator_id, status);
CREATE INDEX IF NOT EXISTS idx_research_sessions_active ON research_sessions(tenant_id, primary_researcher_id, status);
CREATE INDEX IF NOT EXISTS idx_cohort_ingestion_pending ON cohort_ingestion(cohort_name, tenant_slug, status);
CREATE INDEX IF NOT EXISTS idx_research_cohort_reviewers_user ON research_cohort_reviewers(user_id, cohort_id);
CREATE INDEX IF NOT EXISTS idx_research_cohort_batches_cohort ON research_cohort_batches(cohort_id, tenant_id, batch_number);

INSERT OR REPLACE INTO platform_metadata (key, value, updated_at)
VALUES ('schema_version', 'sqlite-platform-v1', CURRENT_TIMESTAMP);
