// Clinical journal privacy controls.
class ClinicalNotesPrivacy {
    constructor(patientId) {
        this.patientId = patientId;
        this.clinicalNotesElement = null;
        this.overlayElement = null;
        this.unblurButton = null;
    }

    async init() {
        this.clinicalNotesElement = document.getElementById('clinicalNotes');
        this.overlayElement = document.getElementById('clinicalNotesOverlay');
        this.unblurButton = document.getElementById('unblurClinicalNotes');

        if (!this.clinicalNotesElement || !this.overlayElement || !this.unblurButton) {
            throw new Error('Clinical journal privacy controls are missing from the patient page');
        }

        this.showOverlay();
        this.setupEventListeners();
    }

    setupEventListeners() {
        if (this.unblurButton) {
            this.unblurButton.addEventListener('click', () => this.unblurNotes());
        }
    }

    unblurNotes() {
        this.hideOverlay();
    }

    showOverlay() {
        if (this.clinicalNotesElement && this.overlayElement) {
            // Clinical notes start blurred by default, so just show overlay
            this.overlayElement.style.display = 'flex';
        }
    }

    hideOverlay() {
        if (this.clinicalNotesElement && this.overlayElement) {
            // Remove default blur and add unblurred class
            this.clinicalNotesElement.classList.add('unblurred');
            this.overlayElement.style.display = 'none';
        }
    }
}

// Administrative review flag controls.
class AdminReviewManager {
    constructor(patientId) {
        this.patientId = patientId;
        this.isFlagged = false;
        this.flagStatus = null;
        this.flagButton = null;
        this.modal = null;
        this.setupModal();
    }

    async init() {
        console.log(`Initializing admin review manager for patient ${this.patientId}`);

        this.flagButton = document.getElementById('flagReviewBtn');

        if (!this.flagButton) {
            console.warn('Flag review button not found in DOM');
            return;
        }

        // Load current admin review status
        await this.loadAdminReviewStatus();

        // Setup event listeners
        this.setupEventListeners();

        // Update UI based on current status
        this.updateUI();
    }

    async loadAdminReviewStatus() {
        const status = await TauriAPI.getAdminReviewStatus(this.patientId);
        if (status && status.status === 'active') {
            this.isFlagged = true;
            this.flagStatus = status;
        } else {
            this.isFlagged = false;
            this.flagStatus = null;
        }
    }

    setupEventListeners() {
        // Flag button click
        this.flagButton.addEventListener('click', () => {
            this.showModal();
        });

        // Modal event listeners
        const closeBtn = document.getElementById('closeAdminModal');
        const cancelBtn = document.getElementById('cancelAdminReview');
        const submitBtn = document.getElementById('submitAdminReview');
        const unflagBtn = document.getElementById('unflagReview');

        if (closeBtn) {
            closeBtn.addEventListener('click', () => this.hideModal());
        }

        if (cancelBtn) {
            cancelBtn.addEventListener('click', () => this.hideModal());
        }

        if (submitBtn) {
            submitBtn.addEventListener('click', () => this.submitReview());
        }

        if (unflagBtn) {
            unflagBtn.addEventListener('click', () => this.unflagPatient());
        }

        // Close modal on overlay click
        this.modal.addEventListener('click', (e) => {
            if (e.target === this.modal) {
                this.hideModal();
            }
        });

        // Close modal on Escape key
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && this.modal.style.display === 'flex') {
                this.hideModal();
            }
        });
    }

    setupModal() {
        this.modal = document.getElementById('adminReviewModal');
    }

    showModal() {
        if (!this.modal) return;

        // Configure modal based on current flag status
        const existingReviewDisplay = document.getElementById('existingReviewDisplay');
        const newReviewForm = document.querySelector('.new-review-form');
        const submitBtn = document.getElementById('submitAdminReview');
        const unflagBtn = document.getElementById('unflagReview');

        if (this.isFlagged && this.flagStatus) {
            const flagType = this.escapeHtml(this.flagStatus.flag_type);
            const flagStatus = this.escapeHtml(this.flagStatus.status);
            const reason = this.escapeHtml(this.flagStatus.reason);
            const createdBy = this.escapeHtml(this.flagStatus.created_by || 'Local operator');
            const createdAt = new Date(this.flagStatus.created_at).toLocaleString();
            existingReviewDisplay.style.display = 'block';
            existingReviewDisplay.innerHTML = `
                <div class="existing-review-info">
                    <h4>This patient is currently flagged for admin review</h4>
                    <div class="review-details">
                        <p><strong>Flag Type:</strong> ${flagType}</p>
                        <p><strong>Status:</strong>
                            <span class="flag-status flag-status-${flagStatus}">${flagStatus}</span>
                        </p>
                        <p><strong>Reason:</strong> ${reason}</p>
                        <p><strong>Flagged by:</strong> ${createdBy}</p>
                        <p><strong>Date:</strong> ${createdAt}</p>
                    </div>
                </div>
            `;

            // Hide form and submit button, show unflag button
            newReviewForm.style.display = 'none';
            submitBtn.style.display = 'none';
            unflagBtn.style.display = 'inline-block';
        } else {
            // Show form for new review
            existingReviewDisplay.style.display = 'none';
            newReviewForm.style.display = 'block';
            submitBtn.style.display = 'inline-block';
            unflagBtn.style.display = 'none';

            // Clear any previous form data
            const reasonTextarea = document.getElementById('reviewReason');
            if (reasonTextarea) {
                reasonTextarea.value = '';
            }
        }

        this.modal.style.display = 'flex';

        // Focus on textarea if showing new review form
        if (!this.isFlagged) {
            const reasonTextarea = document.getElementById('reviewReason');
            if (reasonTextarea) {
                setTimeout(() => reasonTextarea.focus(), 100);
            }
        }
    }

    hideModal() {
        if (this.modal) {
            this.modal.style.display = 'none';
        }

        // Clear any error messages
        const errorMessage = document.getElementById('reviewReasonError');
        if (errorMessage) {
            errorMessage.style.display = 'none';
        }
    }

    escapeHtml(value) {
        const element = document.createElement('div');
        element.textContent = String(value);
        return element.innerHTML;
    }

    async submitReview() {
        const reasonTextarea = document.getElementById('reviewReason');
        const flagTypeSelect = document.getElementById('flagType'); // Enhanced AdminFlag model support
        const errorMessage = document.getElementById('reviewReasonError');

        if (!reasonTextarea || !errorMessage) {
            console.error('Review form elements not found');
            return;
        }

        const reviewReason = reasonTextarea.value.trim();
        if (!flagTypeSelect) {
            throw new Error('Admin review flag type control is missing');
        }
        const flagType = flagTypeSelect.value;

        // Validate input
        if (reviewReason.length < 10) {
            errorMessage.textContent = 'Please provide a detailed reason (at least 10 characters).';
            errorMessage.style.display = 'block';
            reasonTextarea.focus();
            return;
        }

        if (reviewReason.length > 1000) {
            errorMessage.textContent = 'Review reason is too long (maximum 1000 characters).';
            errorMessage.style.display = 'block';
            reasonTextarea.focus();
            return;
        }

        try {
            const createdFlag = await TauriAPI.flagForAdminReview(
                this.patientId,
                reviewReason,
                flagType
            );

            this.isFlagged = true;
            this.flagStatus = createdFlag;

            // Update UI
            this.updateUI();
            this.hideModal();

            // Show success notification
            ToastNotification.success(`Patient flagged for ${flagType.replace('_', ' ')} successfully`);

            console.log(`Successfully flagged patient ${this.patientId} for ${flagType}`);
        } catch (error) {
            console.error('Failed to flag patient for admin review:', error);
            ToastNotification.error('Failed to flag patient for review. Please try again.');
        }
    }

    async unflagPatient() {
        try {
            // Remove flag from backend with resolution note
            const resolutionNote = `Flag resolved by user at ${new Date().toISOString()}`;
            await TauriAPI.clearAdminReviewFlag(this.patientId, resolutionNote);

            // Update local state
            this.isFlagged = false;
            this.flagStatus = null;

            // Update UI
            this.updateUI();
            this.hideModal();

            // Show success notification
            ToastNotification.success('Admin review flag removed successfully');

            console.log(`Successfully removed admin review flag for patient ${this.patientId}`);
        } catch (error) {
            console.error('Failed to remove admin review flag:', error);
            ToastNotification.error('Failed to remove review flag. Please try again.');
        }
    }

    updateUI() {
        if (!this.flagButton) return;

        if (this.isFlagged) {
            this.flagButton.textContent = 'Flagged for Review';
            this.flagButton.style.background = 'hsl(var(--destructive))';
            this.flagButton.style.color = 'white';
            this.flagButton.style.borderColor = 'hsl(var(--destructive))';
        } else {
            this.flagButton.textContent = 'Flag for Review';
            this.flagButton.style.background = '';
            this.flagButton.style.color = '';
            this.flagButton.style.borderColor = '';
        }

        // Show the button now that it's configured
        this.flagButton.style.display = 'inline-block';
    }
}

// Main application initialization
class PatientPageManager {
    constructor() {
        this.patientId = null;
        this.patient = null;
        this.clinicalNotesPrivacy = null;
        this.adminReviewManager = null;
        this.judgmentManager = new TauriJudgmentManager();
        this.isSaving = false; // Flag to prevent concurrent judgment saves
    }

    async init() {
        // Get patient ID from URL
        const urlParams = new URLSearchParams(window.location.search);
        this.patientId = urlParams.get('id');

        if (!this.patientId) {
            console.error('No patient ID provided in URL');
            return;
        }

        if (this.patientId === '[object Object]') {
            this.showLoadingError(new Error(
                'Invalid patient link detected. A patient object was serialized into the URL instead of a patient ID.'
            ));
            return;
        }

        console.log(`Initializing patient page for ID: ${this.patientId}`);

        // Load patient data
        await this.loadPatientData();

        // Initialize features
        await this.initializeFeatures();

        // Setup judgment system
        await this.setupJudgmentSystem();

        console.log('Patient page initialization complete');
    }

    async loadPatientData() {
        console.log(`🔍 PatientPageManager: Loading patient data for ${this.patientId}`);

        // Single strategy: Get complete patient details from backend (backend provides all needed info)
        try {
            console.log('🔍 Loading complete patient details from backend...');
            const patientDetails = await TauriAPI.getPatientDetails(this.patientId);

            this.patient = patientDetails.patient;
            this.judgment = patientDetails.judgment; // Use judgment from patient details response

            this.navigationState = patientDetails.navigation_state;

            this.isInActiveBatch = this.navigationState?.is_in_active_chunk ?? false;

            console.log(`✅ Patient loaded successfully - In batch: ${this.isInActiveBatch}`);

            this.populatePatientData();
            this.setupNavigation();

            // Show out-of-batch indicator if needed
            if (!this.isInActiveBatch) {
                this.showOutOfBatchIndicator();
            }

        } catch (error) {
            console.error('❌ Patient loading failed:', error);
            this.showLoadingError(error);
        }
    }

    setupNavigation() {
        console.log('🧭 Setting up navigation with new navigation state:', this.navigationState);

        if (this.navigationState) {
            this.setupNewNavigation();
        } else {
            this.disableNavigation();
        }
    }

    setupNewNavigation() {
        console.log('🆕 Setting up new navigation system');

        // Update navigation UI elements based on navigation state
        this.updateNavigationUI();

        // Setup navigation button click handlers
        this.setupNavigationHandlers();
    }

    updateNavigationUI() {
        // DUMB FRONTEND: Pure presentation - backend guarantees complete state

        // Previous button - direct assignment from backend
        const prevBtn = document.getElementById('prevPatientBtn');
        prevBtn.disabled = !this.navigationState.previous_button_enabled;
        prevBtn.textContent = this.navigationState.previous_button_text;

        // Next button - direct assignment from backend
        const nextBtn = document.getElementById('nextUnjudgedBtn');
        nextBtn.disabled = !this.navigationState.next_button_enabled;
        nextBtn.textContent = this.navigationState.next_button_text;

        // Counter - direct assignment from backend
        const counter = document.getElementById('patientCounter');
        counter.textContent = this.navigationState.counter_display;

        console.log(`Navigation UI updated - Previous: ${this.navigationState.previous_button_enabled}, Next: ${this.navigationState.next_button_enabled}, Counter: ${this.navigationState.counter_display}`);
    }

    setupNavigationHandlers() {
        // Previous patient button
        const prevPatientBtn = document.getElementById('prevPatientBtn');
        if (prevPatientBtn) {
            prevPatientBtn.addEventListener('click', () => this.handlePreviousClick());
        }

        // Next patient button
        const nextPatientBtn = document.getElementById('nextUnjudgedBtn');
        if (nextPatientBtn) {
            nextPatientBtn.addEventListener('click', () => this.handleNextClick());
        }
    }

    async handlePreviousClick() {
        const previousPatientId = this.navigationState?.previous_patient_id;
        if (!this.navigationState?.previous_button_enabled || !previousPatientId) {
            throw new Error('Previous-patient navigation was requested without a valid target');
        }
        window.location.href = `patient.html?id=${encodeURIComponent(previousPatientId)}`;
    }

    async handleNextClick() {
        console.log('▶️ Next patient clicked');

        // SMART BACKEND: Use the action computed by backend
        if (!this.navigationState || !this.navigationState.next_button_enabled) {
            console.log('❌ Navigation not available or button disabled');
            return;
        }

        const action = this.navigationState.next_button_action;
        console.log(`🎯 Executing backend action: ${action}`);

        try {
            this.setNavigationLoadingState(true);

            switch (action) {
                case 'navigate_next_unjudged':
                    // Get next unjudged patient from backend
                    const nextPatientId = await TauriAPI.getNextUnjudgedPatient(this.patientId);

                    if (nextPatientId) {
                        console.log(`🎯 Navigating to next unjudged patient: ${nextPatientId}`);
                        window.location.href = `patient.html?id=${encodeURIComponent(nextPatientId)}`;
                    } else {
                        console.log('📋 No next unjudged patient available');
                        this.setNavigationLoadingState(false);
                    }
                    break;

                case 'progress_to_next_chunk':
                    console.log('🔄 Progressing to next chunk');
                    const result = await TauriAPI.invoke('progress_to_next_chunk');

                    if (result && result.first_patient_id) {
                        console.log(`🎯 Next chunk loaded, navigating to first patient: ${result.first_patient_id}`);
                        ToastNotification.success(`Loaded next batch! Starting with patient ${result.first_patient_id}`);
                        window.location.href = `patient.html?id=${encodeURIComponent(result.first_patient_id)}`;
                    } else {
                        console.log('✅ Session complete - no more chunks');
                        ToastNotification.success('Research session complete!');
                        window.location.href = 'index.html';
                    }
                    break;

                case 'return_to_index':
                case 'return_to_active_batch':
                    console.log(`🏠 Returning to index page (${action})`);
                    window.location.href = 'index.html';
                    break;

                case 'session_complete':
                    console.log('✅ Session complete');
                    ToastNotification.success('Research session complete!');
                    window.location.href = 'index.html';
                    break;

                case 'review_incomplete':
                default:
                    console.log(`❌ Unhandled or incomplete action: ${action}`);
                    ToastNotification.warning('Review incomplete - please check patient status');
                    this.setNavigationLoadingState(false);
                    break;
            }
        } catch (error) {
            console.error('❌ Failed to handle navigation action:', error);
            ToastNotification.error(`Failed to execute action: ${action}`);
            this.setNavigationLoadingState(false);
        }
    }

    setNavigationLoadingState(loading) {
        // DUMB FRONTEND: Simple loading state management
        const nextPatientBtn = document.getElementById('nextUnjudgedBtn');
        const prevPatientBtn = document.getElementById('prevPatientBtn');

        if (loading) {
            // Set loading state
            if (nextPatientBtn) {
                nextPatientBtn.disabled = true;
                nextPatientBtn.textContent = 'Loading...';
            }
            if (prevPatientBtn) {
                prevPatientBtn.disabled = true;
            }
        } else {
            // Restore from navigation state (backend-computed)
            this.updateNavigationUI();
        }
    }


    showOutOfBatchIndicator() {
        const headerElement = document.getElementById('patientHeader');
        if (headerElement) {
            const indicator = document.createElement('div');
            indicator.className = 'out-of-batch-indicator alert alert-info';
            indicator.innerHTML = `
                <div class="d-flex align-items-center">
                    <i class="fas fa-info-circle me-2"></i>
                    <div>
                        <strong>Outside Active Batch</strong> -
                        You're viewing a patient not in your current work batch.
                        <br><small>Navigation is disabled. Judgments can still be saved.</small>
                    </div>
                    <button class="btn btn-outline-primary btn-sm ms-auto" onclick="window.location.href='index.html'">
                        <i class="fas fa-arrow-left"></i> Return to Active Batch
                    </button>
                </div>
            `;
            headerElement.insertBefore(indicator, headerElement.firstChild);
        }
    }

    disableNavigation() {
        // Hide or disable navigation controls
        const navElements = document.querySelectorAll('.patient-navigation, .nav-button, #patientCounter');
        navElements.forEach(element => {
            element.style.display = 'none';
        });

        // Add a placeholder where navigation would be
        const navContainer = document.querySelector('.navigation-container');
        if (navContainer) {
            navContainer.innerHTML = `
                <div class="navigation-disabled text-muted text-center p-3">
                    <i class="fas fa-ban"></i> Navigation disabled for out-of-batch patients
                </div>
            `;
        }
    }


    showLoadingError(error) {
        let errorMessage = error.message || 'Unknown error occurred';

        // Provide helpful error messages based on error type
        if (errorMessage.includes('not found')) {
            errorMessage = `Patient ${this.patientId} was not found in the system.`;
        }

        document.getElementById('patientHeader').innerHTML = `
            <div class="alert alert-danger">
                <h3><i class="fas fa-exclamation-triangle"></i> Failed to Load Patient Data</h3>
                <p>${errorMessage}</p>
                <div class="mt-3">
                    <a href="index.html" class="btn btn-primary">
                        <i class="fas fa-arrow-left"></i> Return to Main Page
                    </a>
                    <button class="btn btn-outline-secondary ms-2" onclick="window.location.reload()">
                        <i class="fas fa-redo"></i> Try Again
                    </button>
                </div>
            </div>
        `;
    }

    populatePatientData() {
        // Patient header
        const headerElement = document.getElementById('patientHeader');
        if (headerElement && this.patient) {
            headerElement.innerHTML = `
                <div class="patient-basic-info">
                    <h2>Patient ${this.patient.id}</h2>
                    <div class="patient-demographics">
                        <span class="age">Age: ${this.patient.age || 'N/A'}</span>
                        <span class="sex">Sex: ${this.patient.sex || 'N/A'}</span>
                    </div>
                </div>
            `;
        }

        // Clinical data sections
        this.populateSection('pastHistory', this.patient.pastHistory);
        this.populateSection('medication', this.patient.medication);
        this.populateSection('allergies', this.patient.allergies);
        this.populateSection('recentHistory', this.patient.recentHistory);
        this.populateSection('medicalExamination', this.patient.medicalExamination);
        this.populateClinicalJournal(this.patient.clinicalJournal);
    }

    populateSection(sectionId, data) {
        const element = document.getElementById(sectionId);
        if (!element) return;

        if (!data || data.length === 0) {
            element.textContent = 'No data available';
            return;
        }

        if (Array.isArray(data)) {
            element.textContent = data.map(item => {
                if (typeof item === 'string') return item;
                if (typeof item === 'object') {
                    return Object.values(item).join(': ');
                }
                return String(item);
            }).join('\n');
        } else {
            element.textContent = String(data);
        }
    }

    populateClinicalJournal(journalEntries) {
        const element = document.getElementById('clinicalNotes');
        if (!element) return;

        if (!journalEntries || journalEntries.length === 0) {
            element.innerHTML = '<div class="no-data">No clinical journal entries available</div>';
            return;
        }

        // Create structured journal entries - data is already parsed by backend
        const journalHTML = journalEntries.map((entry, index) => {
            // Entry is already a structured object with role, timestamp, content
            return `
                <div class="journal-entry" data-role="${entry.role.toLowerCase()}">
                    <div class="journal-header">
                        <span class="journal-role">${this.escapeHtml(entry.role)}</span>
                        <span class="journal-timestamp">${this.escapeHtml(entry.timestamp)}</span>
                    </div>
                    <div class="journal-content">
                        ${this.formatJournalContent(entry.content)}
                    </div>
                </div>
            `;
        }).join('');

        element.innerHTML = journalHTML;
    }


    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    formatJournalContent(content) {
        // Escape HTML and preserve line breaks
        const escaped = this.escapeHtml(content);
        // Convert newlines to <br> tags for proper display
        return escaped.replace(/\n/g, '<br>');
    }

    async initializeFeatures() {
        const privacyEnabled = await TauriAPI.isFeatureEnabled('clinical_journal_privacy');
        const adminReviewEnabled = await TauriAPI.isFeatureEnabled('admin_review_flagging');

        if (privacyEnabled) {
            this.clinicalNotesPrivacy = new ClinicalNotesPrivacy(this.patientId);
            await this.clinicalNotesPrivacy.init();
        }

        if (adminReviewEnabled) {
            this.adminReviewManager = new AdminReviewManager(this.patientId);
            await this.adminReviewManager.init();
        }
    }

    async setupJudgmentSystem() {
        // Load existing judgment from patient details response (no separate API call needed)
        console.log(`🔍 Patient ${this.patientId} judgment from patient details:`, this.judgment);
        if (this.judgment) {
            console.log(`✅ Found existing judgment: ${this.judgment}`);
            // Set current judgment in manager - UI reflects judgment state from backend data
            this.judgmentManager.setCurrentJudgment(this.judgment);
        } else {
            console.log(`ℹ️ No judgment found for patient ${this.patientId}`);
            // Ensure UI shows no judgment state
            this.judgmentManager.setCurrentJudgment(null);
        }

        // Setup button event listeners
        const appropriateBtn = document.getElementById('appropriateBtn');
        const notAppropriateBtn = document.getElementById('notAppropriateBtn');

        if (appropriateBtn) {
            appropriateBtn.addEventListener('click', async () => {
                console.log('Appropriate button clicked');
                // Use A/N format that matches backend expectations
                const judgmentValue = 'A'; // Maps to "Accepted" in backend
                await this.saveJudgmentWithButtonState(judgmentValue);
            });
        }

        if (notAppropriateBtn) {
            notAppropriateBtn.addEventListener('click', async () => {
                console.log('Not appropriate button clicked');
                // Use A/N format that matches backend expectations
                const judgmentValue = 'N'; // Maps to "Needs Review" in backend
                await this.saveJudgmentWithButtonState(judgmentValue);
            });
        }
    }

    // Set judgment buttons enabled/disabled state with visual feedback
    setJudgmentButtonsEnabled(enabled, loadingType = null) {
        const appropriateBtn = document.getElementById('appropriateBtn');
        const notAppropriateBtn = document.getElementById('notAppropriateBtn');

        // Store original button text if not already stored
        if (!appropriateBtn.dataset.originalText && appropriateBtn) {
            appropriateBtn.dataset.originalText = appropriateBtn.textContent;
        }
        if (!notAppropriateBtn.dataset.originalText && notAppropriateBtn) {
            notAppropriateBtn.dataset.originalText = notAppropriateBtn.textContent;
        }

        if (appropriateBtn) {
            appropriateBtn.disabled = !enabled;
            if (!enabled) {
                appropriateBtn.style.cursor = 'wait';
                appropriateBtn.style.opacity = '0.6';

                // Show loading state based on which button is being clicked
                if (loadingType === 'appropriate') {
                    appropriateBtn.innerHTML = 'Saving...';
                } else if (loadingType === 'not_appropriate') {
                    // Dim the appropriate button when not_appropriate is loading
                    appropriateBtn.style.opacity = '0.3';
                }
            } else {
                appropriateBtn.style.cursor = 'pointer';
                appropriateBtn.style.opacity = '1';
                appropriateBtn.textContent = appropriateBtn.dataset.originalText || 'Appropriate';
            }
        }

        if (notAppropriateBtn) {
            notAppropriateBtn.disabled = !enabled;
            if (!enabled) {
                notAppropriateBtn.style.cursor = 'wait';
                notAppropriateBtn.style.opacity = '0.6';

                // Show loading state based on which button is being clicked
                if (loadingType === 'not_appropriate') {
                    notAppropriateBtn.innerHTML = 'Saving...';
                } else if (loadingType === 'appropriate') {
                    // Dim the not appropriate button when appropriate is loading
                    notAppropriateBtn.style.opacity = '0.3';
                }
            } else {
                notAppropriateBtn.style.cursor = 'pointer';
                notAppropriateBtn.style.opacity = '1';
                notAppropriateBtn.textContent = notAppropriateBtn.dataset.originalText || 'Not Appropriate';
            }
        }
    }

    async saveJudgmentWithButtonState(judgment) {
        // Prevent concurrent saves
        if (this.isSaving) {
            console.log('Already saving judgment, ignoring duplicate click');
            return;
        }

        this.isSaving = true;

        // Determine which type of judgment is being saved for loading state
        const loadingType = (judgment === 'A' || judgment === 'appropriate' || judgment === 'Accepted')
            ? 'appropriate'
            : 'not_appropriate';

        // Prevent multiple concurrent clicks and show loading state
        this.setJudgmentButtonsEnabled(false, loadingType);

        try {
            await this.saveJudgment(judgment);
        } finally {
            // Always re-enable buttons and clear saving state, even if save fails
            this.isSaving = false;
            this.setJudgmentButtonsEnabled(true);
        }
    }

    async saveJudgment(judgment) {
        try {
            console.log(`Saving judgment: ${judgment} for patient ${this.patientId}`);

            // Get result from judgment manager with success/error handling
            const result = await this.judgmentManager.saveJudgment(this.patientId, judgment);

            if (result.success) {
                console.log(`✅ Judgment save SUCCESS: ${result.judgment} for patient ${result.patientId}`);

                // Immediate UI update - no delay needed since judgment manager already updated
                this.judgmentManager.setCurrentJudgment(result.judgment);
                console.log(`🎯 IMMEDIATE UI UPDATE: Set judgment to ${result.judgment}`);

                // Update navigation state when chunk may have progressed
                if (result.chunkProgressed) {
                    console.log('🎯 Chunk progressed - refreshing navigation state');
                    await this.updateNavigationButtonsAfterJudgment();
                } else {
                    console.log('📍 Chunk still in progress - navigation state unchanged');
                }

                // IMMEDIATE MEMORY UPDATE: Update ResearchChunkManager in-memory data
                if (window.researchChunkManager) {
                    window.researchChunkManager.judgments.set(this.patientId, {
                        patient_id: this.patientId,
                        judgment: judgment,
                        timestamp: new Date().toISOString()
                    });
                    console.log('💾 IMMEDIATE: Updated judgment in ResearchChunkManager memory');
                }

                // Notify other parts of the system (lightweight update - no navigation refresh needed)
                localStorage.setItem('judgmentUpdate', JSON.stringify({
                    patientId: this.patientId,
                    judgment: judgment,
                    timestamp: Date.now()
                }));

                // Use a different event name to avoid triggering navigation refresh
                window.dispatchEvent(new CustomEvent('patientJudgmentUpdated', {
                    detail: { patientId: this.patientId, judgment: judgment, skipNavigation: true }
                }));

                // Show success feedback
                ToastNotification.success(`Judgment "${judgment}" saved successfully`);
                console.log('✅ Judgment saved successfully with UI update');

            } else {
                console.error(`❌ Judgment save FAILED: ${result.error}`);
                ToastNotification.error(`Failed to save judgment: ${result.error}`);

                // Reset any UI changes on failure
                this.judgmentManager.clearJudgment();
            }

        } catch (error) {
            console.error('Failed to save judgment:', error);
            ToastNotification.error('Failed to save judgment. Please try again.');

            // Reset any UI changes on exception
            this.judgmentManager.clearJudgment();
        }
    }

    async updateNavigationButtonsAfterJudgment() {
        console.log('🔄 Updating navigation buttons after judgment save');

        // NEW ARCHITECTURAL APPROACH: Reload navigation state from backend
        try {
            console.log('🔄 Reloading navigation state from backend after judgment...');
            const patientDetails = await TauriAPI.getPatientDetails(this.patientId);

            // Update navigation state from backend
            this.navigationState = patientDetails.navigation_state;

            // Update the UI with new navigation state
            if (this.navigationState) {
                this.updateNavigationUI();
            }

            console.log('✅ Navigation state updated from backend after judgment');
        } catch (error) {
            console.error('❌ Failed to reload navigation state after judgment save:', error);
            // NO FALLBACK: If backend fails, navigation state remains as-is
            // This ensures we don't reintroduce frontend business logic
        }
    }

}

// Global initialization state to prevent double initialization
let patientPageInitialized = false;

// Initialize when application is ready (coordinated with app-init.js)
document.addEventListener('appInitialized', async (event) => {
    if (patientPageInitialized) {
        console.log('Patient page already initialized, skipping appInitialized handler');
        return;
    }

    console.log('Patient page initializing after app initialization complete');
    patientPageInitialized = true;

    // TauriAPI and AppConfig are guaranteed to be available now
    if (typeof TauriAPI === 'undefined') {
        console.error('TauriAPI not available despite app initialization');
        return;
    }

    // Initialize patient page
    const patientPageManager = new PatientPageManager();
    await patientPageManager.init();

    // Make manager globally available for debugging
    window.patientPageManager = patientPageManager;
});
