INSERT OR IGNORE INTO tenants (id, name, slug, settings, metadata)
VALUES (
    X'00000000000000000000000000000001',
    'Example Research Workspace',
    'example-research-workspace',
    '{"default_chunk_size":3,"auto_progress_enabled":true}',
    '{"synthetic":true}'
);

INSERT OR IGNORE INTO users (id, email, local_identifier, first_name, last_name, display_name)
VALUES
    (
        X'00000000000000000000000000000003',
        'example-reviewer@example.invalid',
        'example-reviewer',
        'Example',
        'Reviewer',
        'Example Reviewer'
    ),
    (
        X'00000000000000000000000000000004',
        'example-admin@example.invalid',
        'example-admin',
        'Example',
        'Administrator',
        'Example Administrator'
    );

INSERT OR IGNORE INTO user_tenant_roles (id, user_id, tenant_id, role)
VALUES
    (
        X'00000000000000000000000000000101',
        X'00000000000000000000000000000003',
        X'00000000000000000000000000000001',
        'reviewer'
    ),
    (
        X'00000000000000000000000000000102',
        X'00000000000000000000000000000004',
        X'00000000000000000000000000000001',
        'admin'
    );

INSERT OR IGNORE INTO patients (id, external_id, age, sex, tenant_id, review_status, priority_level)
VALUES
    (
        X'00000000000000000000000000000201',
        'SYNTH-001',
        34,
        'F',
        X'00000000000000000000000000000001',
        'pending',
        1
    ),
    (
        X'00000000000000000000000000000202',
        'SYNTH-002',
        57,
        'M',
        X'00000000000000000000000000000001',
        'pending',
        2
    ),
    (
        X'00000000000000000000000000000203',
        'SYNTH-003',
        71,
        'F',
        X'00000000000000000000000000000001',
        'pending',
        1
    );

INSERT OR IGNORE INTO research_cohorts (
    id,
    tenant_id,
    name,
    description,
    total_patients,
    cohort_type,
    created_by,
    status,
    metadata
)
VALUES (
    X'00000000000000000000000000000301',
    X'00000000000000000000000000000001',
    'Synthetic Review Cohort',
    'Fictional cohort used only for automated validation',
    3,
    'imported',
    X'00000000000000000000000000000004',
    'active',
    '{"synthetic":true}'
);

INSERT OR IGNORE INTO research_cohort_reviewers (
    cohort_id,
    user_id,
    role,
    can_review,
    can_export,
    can_modify_cohort,
    granted_by,
    access_metadata
)
VALUES (
    X'00000000000000000000000000000301',
    X'00000000000000000000000000000003',
    'reviewer',
    1,
    0,
    0,
    X'00000000000000000000000000000004',
    '{"synthetic":true}'
);

INSERT OR IGNORE INTO research_cohort_patients (
    cohort_id,
    patient_id,
    display_order,
    inclusion_reason,
    added_by
)
VALUES
    (
        X'00000000000000000000000000000301',
        X'00000000000000000000000000000201',
        1,
        'Synthetic validation record',
        X'00000000000000000000000000000004'
    ),
    (
        X'00000000000000000000000000000301',
        X'00000000000000000000000000000202',
        2,
        'Synthetic validation record',
        X'00000000000000000000000000000004'
    ),
    (
        X'00000000000000000000000000000301',
        X'00000000000000000000000000000203',
        3,
        'Synthetic validation record',
        X'00000000000000000000000000000004'
    );

INSERT OR IGNORE INTO research_cohort_batches (
    id,
    cohort_id,
    tenant_id,
    batch_number,
    patient_external_ids,
    is_empty
)
VALUES (
    X'00000000000000000000000000000401',
    X'00000000000000000000000000000301',
    X'00000000000000000000000000000001',
    1,
    '["SYNTH-001","SYNTH-002","SYNTH-003"]',
    0
);
