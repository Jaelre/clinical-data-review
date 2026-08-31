// Cohort Manager - handles cohort discovery, selection, and session creation
// Integrates with backend cohort service for database-centric workflow

class CohortManager {
    constructor() {
        this.availableCohorts = [];
        this.selectedCohort = null;
        this.lastCohortsRefresh = 0;
        this.refreshDebounceMs = 5000;

        this.ready = this.init();
    }

    async init() {
        console.log('🚀 Initializing Cohort Manager...');

        // Wait for TauriAPI to be available
        await this.waitForTauriAPI();

        // Load available cohorts from backend
        await this.loadAvailableCohorts();

        // Setup event listeners
        this.setupEventListeners();

        // Render cohort selection interface
        this.renderCohortInterface();

        console.log('✅ Cohort Manager initialized');
    }

    async waitForTauriAPI() {
        let attempts = 0;
        const maxAttempts = 50; // 5 seconds max wait

        while (attempts < maxAttempts) {
            if (typeof TauriAPI !== 'undefined') {
                console.log('✅ TauriAPI is ready for cohorts');
                return;
            }

            await new Promise(resolve => setTimeout(resolve, 100));
            attempts++;
        }

        throw new Error('Tauri API was not available within five seconds');
    }

    // Load available cohorts from backend service
    async loadAvailableCohorts() {
        this.availableCohorts = await TauriAPI.getAvailableCohorts();
        this.lastCohortsRefresh = Date.now();
    }

    // Refresh cohorts data with debouncing
    async refreshCohorts() {
        const now = Date.now();
        if (now - this.lastCohortsRefresh < this.refreshDebounceMs) {
            console.log('🚫 Cohorts refresh debounced');
            return;
        }

        await this.loadAvailableCohorts();
        this.renderCohortInterface();
    }

    // Setup event listeners
    setupEventListeners() {
        // Listen for visibility changes to refresh data
        document.addEventListener('visibilitychange', () => {
            if (!document.hidden) {
                this.refreshCohorts();
            }
        });

        // Keyboard shortcut for manual refresh (Ctrl+Shift+C)
        document.addEventListener('keydown', (e) => {
            if (e.ctrlKey && e.shiftKey && e.key === 'C') {
                e.preventDefault();
                console.log('🔄 Manual cohorts refresh triggered');
                this.refreshCohorts();
            }
        });
    }

    // Render the cohort selection interface
    renderCohortInterface() {
        const container = document.getElementById('patientAccordion');
        if (!container) {
            console.warn('Patient accordion container not found for cohorts');
            return;
        }

        // Hide loading state
        const loadingState = document.getElementById('loadingState');
        if (loadingState) {
            loadingState.style.display = 'none';
        }

        if (!this.availableCohorts || this.availableCohorts.length === 0) {
            container.innerHTML = this.renderNoCohorts();
            return;
        }

        container.innerHTML = this.renderCohortSelection();
        this.setupCohortSelectionEvents();
    }

    // Render no cohorts available state
    renderNoCohorts() {
        return `
            <div class="no-cohorts-available">
                <h3>No Research Cohorts Available</h3>
                <p>No research cohorts are currently available for review.</p>
                <ul>
                    <li>Run the documented local ETL command with PII purging.</li>
                    <li>Import a cohort for an existing local operator.</li>
                    <li>Refresh this page after the import completes.</li>
                </ul>

                <div class="cohorts-help">
                    <h4>What You Need</h4>
                    <p>This environment expects a prepared clinical workspace database that already contains cohort definitions and patient assignments. For local development, import or seed sample cohort data before retrying.</p>
                </div>

                <div class="cohorts-help">
                    <h4>What are Research Cohorts?</h4>
                    <p>Research cohorts are pre-defined groups of patients organized for specific studies or reviews.
                    They replace manual patient file loading with structured database-managed patient collections.</p>
                </div>

                <div class="refresh-section">
                    <button id="refreshCohortsBtn" class="btn-secondary">
                        🔄 Refresh Cohorts
                    </button>
                </div>
            </div>
        `;
    }

    // Render cohort selection interface
    renderCohortSelection() {
        return `
            <div class="cohort-selection-interface">
                <div class="cohort-selection-header">
                    <h3>Select Research Cohort</h3>
                    <p>Choose a research cohort to begin your clinical data review session.</p>
                    <button id="refreshCohortsBtn" class="btn-refresh">🔄 Refresh</button>
                </div>

                <div class="cohorts-grid">
                    ${this.availableCohorts.map(cohort => this.renderCohortCard(cohort)).join('')}
                </div>

                <div class="cohort-help-section">
                    <h4>About Research Cohorts</h4>
                    <p>Each cohort represents a curated group of patients for review. Select a cohort to start a new review session
                    with those patients organized into manageable chunks.</p>
                </div>
            </div>
        `;
    }

    // Render individual cohort card
    renderCohortCard(cohort) {
        const canReview = cohort.can_review;
        const cardClass = canReview ? 'cohort-card selectable' : 'cohort-card disabled';

        return `
            <div class="${cardClass}" data-cohort-id="${cohort.id}">
                <div class="cohort-header">
                    <h4 class="cohort-name">${this.escapeHtml(cohort.name)}</h4>
                    <div class="cohort-stats">
                        <span class="patient-count">${cohort.total_patients} patients</span>
                        <span class="cohort-type">${this.escapeHtml(cohort.cohort_type)}</span>
                    </div>
                </div>

                <div class="cohort-body">
                    ${cohort.description ?
                        `<p class="cohort-description">${this.escapeHtml(cohort.description)}</p>` :
                        '<p class="cohort-description">No description available</p>'
                    }

                    <div class="cohort-metadata">
                        <div class="metadata-item">
                            <span class="label">Your Role:</span>
                            <span class="value">${this.escapeHtml(cohort.user_role)}</span>
                        </div>
                        <div class="metadata-item">
                            <span class="label">Status:</span>
                            <span class="value status-${cohort.status}">${this.escapeHtml(cohort.status)}</span>
                        </div>
                        <div class="metadata-item">
                            <span class="label">Created:</span>
                            <span class="value">${this.formatDate(cohort.created_at)}</span>
                        </div>
                    </div>

                    <div class="cohort-permissions">
                        <span class="permission ${cohort.can_review ? 'granted' : 'denied'}">
                            ${cohort.can_review ? 'Review Access: Enabled' : 'Review Access: Disabled'}
                        </span>
                        <span class="permission ${cohort.can_export ? 'granted' : 'denied'}">
                            ${cohort.can_export ? 'Export Access: Enabled' : 'Export Access: Disabled'}
                        </span>
                    </div>
                </div>

                <div class="cohort-actions">
                    ${canReview ?
                        `<button class="btn-primary start-review-btn" data-cohort-id="${cohort.id}">
                            Start Review Session
                        </button>` :
                        `<button class="btn-disabled" disabled>
                            Review Permission Required
                        </button>`
                    }
                </div>
            </div>
        `;
    }

    // Setup event handlers for cohort selection
    setupCohortSelectionEvents() {
        // Refresh button
        const refreshBtn = document.getElementById('refreshCohortsBtn');
        if (refreshBtn) {
            refreshBtn.addEventListener('click', async (e) => {
                e.preventDefault();
                await this.refreshCohorts();
            });
        }

        // Start review session buttons
        document.querySelectorAll('.start-review-btn').forEach(btn => {
            btn.addEventListener('click', async (e) => {
                e.preventDefault();
                const cohortId = btn.getAttribute('data-cohort-id');
                if (cohortId) {
                    await this.startReviewSession(cohortId);
                }
            });
        });

        // Cohort card hover effects
        document.querySelectorAll('.cohort-card.selectable').forEach(card => {
            card.addEventListener('mouseenter', () => {
                card.classList.add('hovered');
            });
            card.addEventListener('mouseleave', () => {
                card.classList.remove('hovered');
            });
        });
    }

    // Start a review session for the selected cohort
    async startReviewSession(cohortId) {
        try {
            console.log(`🚀 Starting review session for cohort: ${cohortId}`);

            // Find the cohort details
            const cohort = this.availableCohorts.find(c => c.id === cohortId);
            if (!cohort) {
                throw new Error('Cohort not found');
            }

            // Show loading state
            this.showSessionCreationModal(cohort);

        } catch (error) {
            console.error('❌ Failed to start review session:', error);
            this.showError(`Failed to start review session: ${error.message || error}`);
        }
    }

    // Show session creation modal
    showSessionCreationModal(cohort) {
        const modal = document.createElement('div');
        modal.className = 'session-creation-modal';
        modal.innerHTML = `
            <div class="modal-backdrop"></div>
            <div class="modal-content">
                <div class="modal-header">
                    <h3>Create Review Session</h3>
                    <button class="modal-close" id="closeSessionModal">×</button>
                </div>

                <div class="modal-body">
                    <div class="cohort-summary">
                        <h4>${this.escapeHtml(cohort.name)}</h4>
                        <p>${cohort.total_patients} patients available for review</p>
                    </div>

                    <div class="session-config">
                        <div class="form-group">
                            <label for="sessionName">Session Name:</label>
                            <input type="text" id="sessionName" class="form-input"
                                   placeholder="Review Session for ${cohort.name}"
                                   value="Review Session: ${cohort.name}">
                        </div>

                        <div class="form-group">
                            <label>Batching:</label>
                            <div class="form-help">
                                Review batches are pre-composed during ETL ingestion. Starting a session opens your own progress against those existing batches.
                            </div>
                        </div>
                    </div>

                    <div id="sessionCreationStatus" class="creation-status" style="display: none;"></div>
                </div>

                <div class="modal-actions">
                    <button id="cancelSessionBtn" class="btn-secondary">Cancel</button>
                    <button id="createSessionBtn" class="btn-primary">
                        Create Session
                    </button>
                </div>
            </div>
        `;

        document.body.appendChild(modal);

        // Setup modal event handlers
        this.setupSessionModalEvents(modal, cohort);
    }

    // Setup session creation modal events
    setupSessionModalEvents(modal, cohort) {
        const closeBtn = modal.querySelector('#closeSessionModal');
        const cancelBtn = modal.querySelector('#cancelSessionBtn');
        const createBtn = modal.querySelector('#createSessionBtn');
        const backdrop = modal.querySelector('.modal-backdrop');

        // Close modal handlers
        const closeModal = () => {
            modal.remove();
        };

        closeBtn.addEventListener('click', closeModal);
        cancelBtn.addEventListener('click', closeModal);
        backdrop.addEventListener('click', closeModal);

        // Create session handler
        createBtn.addEventListener('click', async (e) => {
            e.preventDefault();

            const sessionNameInput = modal.querySelector('#sessionName');
            const statusDiv = modal.querySelector('#sessionCreationStatus');

            const sessionName = sessionNameInput.value?.trim();

            if (!sessionName) {
                this.showModalStatus(statusDiv, 'Please enter a session name', 'error');
                return;
            }

            try {
                // Disable button and show loading
                createBtn.disabled = true;
                createBtn.innerHTML = 'Creating Session...';
                this.showModalStatus(statusDiv, 'Creating review session...', 'info');

                console.log(`🚀 Creating review session for cohort ${cohort.id}: "${sessionName}" using ETL-authored batches`);

                // Call backend to start review session
                const response = await TauriAPI.startReviewSessionForCohort(
                    cohort.id,
                    sessionName
                );

                console.log('✅ Review session created successfully:', response);
                this.showModalStatus(statusDiv, 'Session created successfully! Redirecting...', 'success');

                // Wait a moment then close modal and refresh interface
                setTimeout(() => {
                    closeModal();
                    // Notify the research chunk manager to refresh
                    if (window.researchChunkManager) {
                        window.researchChunkManager.forceProgressUpdate();
                    }
                    // Could also redirect to patient review interface here if needed
                }, 2000);

            } catch (error) {
                console.error('❌ Failed to create review session:', error);

                let errorMessage = 'Unknown error occurred';
                if (error && typeof error === 'object') {
                    if (error.NotFound) {
                        errorMessage = `Cohort not found: ${error.NotFound.identifier}`;
                    } else if (error.InvalidInput) {
                        errorMessage = `Invalid input: ${error.InvalidInput.message || JSON.stringify(error.InvalidInput)}`;
                    } else if (error.DataAccess) {
                        errorMessage = `Database error: ${error.DataAccess.message || JSON.stringify(error.DataAccess)}`;
                    } else {
                        errorMessage = error.message || error.error || JSON.stringify(error);
                    }
                } else if (typeof error === 'string') {
                    errorMessage = error;
                }

                this.showModalStatus(statusDiv, `Failed to create session: ${errorMessage}`, 'error');

                // Re-enable button
                createBtn.disabled = false;
                createBtn.innerHTML = 'Create Session';
            }
        });
    }

    // Show status in modal
    showModalStatus(statusDiv, message, type) {
        if (!statusDiv) return;

        statusDiv.style.display = 'block';
        statusDiv.className = `creation-status ${type}`;
        statusDiv.textContent = message;

        // Auto-hide success/info messages
        if (type === 'success' || type === 'info') {
            setTimeout(() => {
                try {
                    if (statusDiv && statusDiv.parentNode) {
                        statusDiv.style.display = 'none';
                    }
                } catch (e) {
                    console.warn('Failed to hide modal status:', e);
                }
            }, 5000);
        }
    }

    // Show error message
    showError(message) {
        const container = document.getElementById('patientAccordion');
        if (!container) return;

        const errorDiv = document.createElement('div');
        errorDiv.className = 'error-message';
        errorDiv.innerHTML = `
            <div class="error-content">
                <h4>Error</h4>
                <p>${this.escapeHtml(message)}</p>
                <button class="btn-secondary" onclick="this.parentElement.parentElement.remove()">
                    Dismiss
                </button>
            </div>
        `;

        container.insertBefore(errorDiv, container.firstChild);

        // Auto-remove after 10 seconds
        setTimeout(() => {
            try {
                if (errorDiv && errorDiv.parentNode) {
                    errorDiv.remove();
                }
            } catch (e) {
                console.warn('Failed to auto-remove error:', e);
            }
        }, 10000);
    }

    // Utility methods
    escapeHtml(text) {
        if (!text) return '';
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    formatDate(dateString) {
        try {
            const date = new Date(dateString);
            return date.toLocaleDateString('en-US', {
                year: 'numeric',
                month: 'short',
                day: 'numeric'
            });
        } catch (e) {
            return 'Invalid date';
        }
    }

    // Public API for manual refresh
    async refreshCohortsData() {
        console.log('🔄 Manual cohorts refresh requested');
        await this.refreshCohorts();
    }

}

window.CohortManager = CohortManager;
