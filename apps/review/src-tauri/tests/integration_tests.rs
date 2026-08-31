use clinical_data_review::AppState;
use platform_db::{DatabaseConnection, DatabaseConnectionType};
use review_core::admin_flag::service::AdminFlagService;
use review_core::cohort::service::CohortService;
use review_core::judgment::service::JudgmentService;
use review_core::patient::service::PatientService;
use review_core::research_session::service::ResearchSessionService;
use review_core::Config;
use std::ops::{Deref, DerefMut};

struct TestAppState {
    state: AppState,
    _directory: tempfile::TempDir,
}

impl Deref for TestAppState {
    type Target = AppState;

    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for TestAppState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

async fn create_unselected_state() -> TestAppState {
    let directory = tempfile::tempdir().expect("temporary integration database directory");
    let database_path = directory.path().join("review.sqlite3");
    let database_url = format!("sqlite://{}", database_path.display());

    let db = DatabaseConnectionType::new(&database_url)
        .await
        .expect("failed to open sqlite integration database");
    db.run_migrations()
        .await
        .expect("failed to migrate sqlite integration database");
    db.seed_synthetic_fixture_data()
        .await
        .expect("failed to seed synthetic integration data");

    let state = AppState::new_with_database_url(Config::default(), &database_url)
        .await
        .expect("failed to create app state");
    TestAppState {
        state,
        _directory: directory,
    }
}

async fn create_state_with_operator(operator_name: &str) -> TestAppState {
    let mut state = create_unselected_state().await;

    state
        .create_local_operator(operator_name)
        .await
        .expect("failed to create local operator");

    state
}

#[tokio::test]
async fn test_local_operator_session_flow() {
    let mut state = create_state_with_operator("Integration Reviewer Alpha").await;

    assert!(state.has_active_operator_session());
    assert!(state.get_current_user_id().is_some());
    assert!(state.get_current_tenant_id().is_some());

    let operators = state
        .list_local_operators()
        .await
        .expect("failed to list local operators");
    assert!(
        operators
            .iter()
            .any(|operator| operator.display_name == "Integration Reviewer Alpha"),
        "expected the newly created operator to be discoverable from the local mask"
    );

    state
        .clear_operator_session()
        .await
        .expect("failed to clear operator session");
    assert!(!state.has_active_operator_session());
}

#[tokio::test]
async fn test_clear_operator_session_ends_persisted_work_session() {
    let mut state = create_state_with_operator("Integration Reviewer Delta").await;

    let operator_id = state
        .get_current_user_id()
        .expect("operator session should be active");
    let tenant_id = state
        .get_current_tenant_id()
        .expect("operator should have a local workspace tenant");

    assert!(
        state
            .db
            .get_active_local_work_session(tenant_id, operator_id)
            .await
            .expect("failed to inspect work session before clearing")
            .is_some(),
        "creating a local operator should create an active persisted work session"
    );

    state
        .clear_operator_session()
        .await
        .expect("failed to clear operator session");

    assert!(
        state
            .db
            .get_active_local_work_session(tenant_id, operator_id)
            .await
            .expect("failed to inspect work session after clearing")
            .is_none(),
        "clearing the UI session should also end the persisted local work session"
    );
}

#[tokio::test]
async fn test_operator_can_start_session_from_ingested_cohort() {
    let state = create_state_with_operator("Integration Reviewer Bravo").await;

    let operator_id = state
        .get_current_user_id()
        .expect("operator session should be active");
    let tenant_id = state
        .get_current_tenant_id()
        .expect("operator should have a local workspace tenant");

    let cohort_service = CohortService::new(&*state.db, tenant_id, Config::default());
    let cohorts = cohort_service
        .get_available_cohorts(operator_id)
        .await
        .expect("failed to load cohorts");

    assert!(
        !cohorts.is_empty(),
        "expected ingested cohorts to be available for the local operator"
    );

    let session_response = cohort_service
        .start_review_session_for_cohort(cohorts[0].id, operator_id, None)
        .await
        .expect("failed to start cohort review session");

    assert!(
        !session_response.session.current_chunk_patients.is_empty(),
        "review session should load an initial patient chunk"
    );

    let patient_service = PatientService::new(&*state.db, tenant_id, Config::default());
    let patients = patient_service
        .get_all_patients_for_researcher(operator_id)
        .await
        .expect("failed to load patients for active review session");

    assert_eq!(
        patients.len(),
        session_response.session.current_chunk_patients.len(),
        "session-scoped patient loading should match the active chunk"
    );
}

#[tokio::test]
async fn test_next_patient_lookup_uses_operator_session_context() {
    let state = create_state_with_operator("Integration Reviewer Charlie").await;

    let operator_id = state
        .get_current_user_id()
        .expect("operator session should be active");
    let tenant_id = state
        .get_current_tenant_id()
        .expect("operator should have a local workspace tenant");

    let cohort_service = CohortService::new(&*state.db, tenant_id, Config::default());
    let cohorts = cohort_service
        .get_available_cohorts(operator_id)
        .await
        .expect("failed to load cohorts");

    let session_response = cohort_service
        .start_review_session_for_cohort(cohorts[0].id, operator_id, None)
        .await
        .expect("failed to start cohort review session");

    let current_chunk = &session_response.session.current_chunk_patients;
    assert!(
        current_chunk.len() >= 2,
        "expected fixture cohort to populate at least two patients in the active chunk"
    );

    let service = ResearchSessionService::new(&*state.db, tenant_id, Config::default());
    let next_patient = service
        .get_next_unjudged_patient_id(&current_chunk[0], operator_id)
        .await
        .expect("failed to resolve next patient in session");

    assert_eq!(next_patient, Some(current_chunk[1].clone()));
}

#[tokio::test]
async fn test_next_patient_lookup_wraps_to_earlier_unjudged_patient_in_batch() {
    let state = create_state_with_operator("Integration Reviewer Echo").await;

    let operator_id = state
        .get_current_user_id()
        .expect("operator session should be active");
    let tenant_id = state
        .get_current_tenant_id()
        .expect("operator should have a local workspace tenant");

    let cohort_service = CohortService::new(&*state.db, tenant_id, Config::default());
    let cohorts = cohort_service
        .get_available_cohorts(operator_id)
        .await
        .expect("failed to load cohorts");

    let session_response = cohort_service
        .start_review_session_for_cohort(cohorts[0].id, operator_id, None)
        .await
        .expect("failed to start cohort review session");

    let current_chunk = &session_response.session.current_chunk_patients;
    assert!(
        current_chunk.len() >= 3,
        "expected fixture cohort to populate at least three patients in the active chunk"
    );

    let judgment_service = JudgmentService::new(&*state.db, tenant_id, Config::default());
    judgment_service
        .save_judgment(&current_chunk[0], "A", Some(operator_id))
        .await
        .expect("failed to save judgment for first patient");
    judgment_service
        .save_judgment(&current_chunk[2], "N", Some(operator_id))
        .await
        .expect("failed to save judgment for last patient");

    let service = ResearchSessionService::new(&*state.db, tenant_id, Config::default());
    let next_patient = service
        .get_next_unjudged_patient_id(&current_chunk[2], operator_id)
        .await
        .expect("failed to resolve wrapped next patient in session");

    assert_eq!(
        next_patient,
        Some(current_chunk[1].clone()),
        "next-patient lookup should wrap to earlier unjudged patients in the same batch"
    );
}

#[tokio::test]
async fn test_session_summary_reports_active_chunk_and_session_counts() {
    let state = create_state_with_operator("Integration Reviewer Foxtrot").await;

    let operator_id = state
        .get_current_user_id()
        .expect("operator session should be active");
    let tenant_id = state
        .get_current_tenant_id()
        .expect("operator should have a local workspace tenant");

    let cohort_service = CohortService::new(&*state.db, tenant_id, Config::default());
    let cohorts = cohort_service
        .get_available_cohorts(operator_id)
        .await
        .expect("failed to load cohorts");

    let session_response = cohort_service
        .start_review_session_for_cohort(cohorts[0].id, operator_id, None)
        .await
        .expect("failed to start cohort review session");

    let current_chunk = &session_response.session.current_chunk_patients;
    assert!(
        !current_chunk.is_empty(),
        "expected fixture cohort to populate an active chunk"
    );

    let judgment_service = JudgmentService::new(&*state.db, tenant_id, Config::default());
    judgment_service
        .save_judgment(&current_chunk[0], "A", Some(operator_id))
        .await
        .expect("failed to save judgment for first patient");

    let research_service = ResearchSessionService::new(&*state.db, tenant_id, Config::default());
    let summary = research_service
        .get_session_summary(operator_id)
        .await
        .expect("failed to load session summary")
        .expect("expected an active session summary");

    let active_chunk = summary
        .active_chunk
        .expect("expected active chunk summary to be present");

    assert_eq!(
        active_chunk.id,
        session_response.session.current_chunk_number
    );
    assert_eq!(active_chunk.total_patients, current_chunk.len());
    assert!(active_chunk.completed_patients >= 1);
    assert_eq!(
        summary.total_patients,
        session_response.session.total_patients as usize
    );
    assert!(summary.judged_patients >= 1);
}

#[tokio::test]
async fn test_judgment_summary_for_active_session_returns_real_counts_and_activity() {
    let state = create_state_with_operator("Integration Reviewer Golf").await;

    let operator_id = state
        .get_current_user_id()
        .expect("operator session should be active");
    let tenant_id = state
        .get_current_tenant_id()
        .expect("operator should have a local workspace tenant");

    let cohort_service = CohortService::new(&*state.db, tenant_id, Config::default());
    let cohorts = cohort_service
        .get_available_cohorts(operator_id)
        .await
        .expect("failed to load cohorts");

    let session_response = cohort_service
        .start_review_session_for_cohort(cohorts[0].id, operator_id, None)
        .await
        .expect("failed to start cohort review session");

    let current_chunk = &session_response.session.current_chunk_patients;
    assert!(
        current_chunk.len() >= 2,
        "expected fixture cohort to populate at least two patients in the active chunk"
    );

    let judgment_service = JudgmentService::new(&*state.db, tenant_id, Config::default());
    judgment_service
        .save_judgment(&current_chunk[0], "A", Some(operator_id))
        .await
        .expect("failed to save accepted judgment");
    judgment_service
        .save_judgment(&current_chunk[1], "N", Some(operator_id))
        .await
        .expect("failed to save needs-review judgment");

    let summary = judgment_service
        .get_judgment_summary_for_researcher(operator_id)
        .await
        .expect("failed to load judgment summary");

    assert!(summary.total_judgments >= 2);
    assert!(summary.accepted_count >= 1);
    assert!(summary.needs_review_count >= 1);
    assert!(summary.recent_judgments.len() >= 2);
    assert!(summary
        .recent_judgments
        .iter()
        .any(|judgment| judgment.patient_id == current_chunk[0] && judgment.judgment == "A"));
    assert!(summary
        .recent_judgments
        .iter()
        .any(|judgment| judgment.patient_id == current_chunk[1] && judgment.judgment == "N"));
}

#[tokio::test]
async fn test_final_judgment_completes_the_cohort_session() {
    let state = create_state_with_operator("Integration Reviewer Hotel").await;
    let operator_id = state
        .get_current_user_id()
        .expect("operator session should be active");
    let tenant_id = state
        .get_current_tenant_id()
        .expect("operator should have a local workspace tenant");

    let cohort_service = CohortService::new(&*state.db, tenant_id, Config::default());
    let cohort = cohort_service
        .get_available_cohorts(operator_id)
        .await
        .expect("load cohorts")
        .into_iter()
        .next()
        .expect("synthetic cohort");
    let session = cohort_service
        .start_review_session_for_cohort(cohort.id, operator_id, None)
        .await
        .expect("start review session")
        .session;

    let judgment_service = JudgmentService::new(&*state.db, tenant_id, Config::default());
    for patient_id in session
        .current_chunk_patients
        .iter()
        .take(session.current_chunk_patients.len().saturating_sub(1))
    {
        judgment_service
            .save_judgment(patient_id, "A", Some(operator_id))
            .await
            .expect("save non-final judgment");
    }

    let final_patient = session
        .current_chunk_patients
        .last()
        .expect("non-empty synthetic batch");
    let progressed = judgment_service
        .save_judgment_and_progress_session(final_patient, "A", Some(operator_id))
        .await
        .expect("save final judgment and complete session");
    assert!(progressed);
    assert!(
        state
            .db
            .get_active_research_session(tenant_id, operator_id)
            .await
            .expect("inspect active session")
            .is_none(),
        "the final cohort batch should close the review session"
    );
}

#[tokio::test]
async fn test_activate_operator_rejects_unknown_operator_id() {
    let mut state = create_unselected_state().await;

    let session_state = state
        .operator_session_state()
        .await
        .expect("failed to read operator session state");
    assert!(session_state.requires_operator_selection);

    let error = state
        .activate_operator(uuid::Uuid::new_v4())
        .await
        .expect_err("unknown operators must not activate a session");

    let message = format!("{error}");
    assert!(
        message.contains("local_operator"),
        "expected a local operator lookup error, got: {message}"
    );
    assert!(!state.has_active_operator_session());
}

#[tokio::test]
async fn test_admin_flag_lifecycle_is_persisted() {
    let state = create_state_with_operator("Integration Reviewer Flags").await;
    let operator_id = state
        .get_current_user_id()
        .expect("active operator identifier");
    let tenant_id = state
        .get_current_tenant_id()
        .expect("active workspace identifier");
    let service = AdminFlagService::new(&*state.db, tenant_id, Config::default());

    let created = service
        .flag_for_admin_review_with_type(
            "SYNTH-001",
            "Synthetic data-quality review",
            "data_quality",
            operator_id,
        )
        .await
        .expect("create admin flag");
    assert_eq!(created.status, "active");

    let summary = service
        .get_admin_flag_statistics()
        .await
        .expect("summarize admin flags");
    assert_eq!(summary.total_flagged_cases, 1);
    assert_eq!(summary.pending_review_count, 1);

    service
        .resolve_admin_flag("SYNTH-001", "Synthetic review complete", operator_id)
        .await
        .expect("resolve admin flag");
    let resolved = service
        .get_admin_flag("SYNTH-001")
        .await
        .expect("load resolved admin flag")
        .expect("resolved flag exists");
    assert_eq!(resolved.status, "resolved");
}
