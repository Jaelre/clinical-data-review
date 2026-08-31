// Research session rendering and local data caching.

class ResearchChunkManager {
    constructor() {
        this.sessionData = null;
        this.cachedPatients = null;
        this.lastDataRefresh = 0;
        this.refreshDebounceMs = 3000;

        this.ready = this.init();
    }

    createEmptySessionData() {
        return {
            active_chunk: null,
            completed_chunks: [],
            total_patients: 0,
            judged_patients: 0
        };
    }

    async getSessionData() {
        return await TauriAPI.getResearchSession(null);
    }

    async init() {
        console.log('🚀 Initializing simplified Research Chunk Manager...');

        // Wait for TauriAPI to be available
        await this.waitForTauriAPI();

        // Load session data once to determine workflow path
        console.log('🚀 Loading research session data to determine workflow...');
        const sessionData = await this.getSessionData();

        // Check if cohorts workflow should be used instead
        if (await this.shouldUseCohortWorkflow(sessionData)) {
            console.log('📋 Cohorts available - initializing CohortManager instead');
            await this.initializeCohortManager();
            return;
        }

        // Load research session data from backend (pass existing data to avoid duplicate call)
        await this.loadResearchSession(sessionData);

        // Setup simple event listeners
        this.setupEventListeners();

        // Render interface using backend data
        this.renderInterface();

        console.log('✅ Research Chunk Manager initialized');
    }

    // Simple backend data loading - delegates all business logic to backend
    async loadResearchSession(existingSessionData = null) {
        try {
            console.log('🚀 Loading research session data from backend...');

            let sessionData = null;
            let cachedPatients = [];

            if (existingSessionData !== null) {
                sessionData = existingSessionData;
            } else {
                sessionData = await this.getSessionData();
            }

            if (!sessionData) {
                sessionData = this.createEmptySessionData();
            }

            if (!sessionData.active_chunk || !sessionData.active_chunk.patient_ids) {
                this.sessionData = sessionData;
                this.cachedPatients = [];
                console.log('ℹ️ No active research session chunk available');
                return;
            }

            console.log(`🎯 Loading ${sessionData.active_chunk.patient_ids.length} patients from research session chunk`);
            cachedPatients = await TauriAPI.loadPatientsByIds(sessionData.active_chunk.patient_ids);
            console.log('👥 Session patients loaded:', cachedPatients?.length || 0, 'patients');

            if (!cachedPatients || cachedPatients.length === 0) {
                throw new Error(`❌ CRITICAL: No patients loaded for session chunk with ${sessionData.active_chunk.patient_ids.length} patient IDs`);
            }

            this.sessionData = sessionData;
            this.cachedPatients = cachedPatients;

            console.log(`✅ Loaded session data and ${this.cachedPatients.length} patients`);
        } catch (error) {
            throw new Error('Failed to load research session data', { cause: error });
        }
    }

    async waitForTauriAPI() {
        let attempts = 0;
        const maxAttempts = 50; // 5 seconds max wait

        while (attempts < maxAttempts) {
            if (typeof TauriAPI !== 'undefined') {
                console.log('✅ TauriAPI is ready');
                return;
            }

            await new Promise(resolve => setTimeout(resolve, 100));
            attempts++;
        }

        throw new Error('Tauri API was not available within five seconds');
    }

    // Simple data refresh - delegates to backend services
    async refreshData() {
        const now = Date.now();
        if (now - this.lastDataRefresh < this.refreshDebounceMs) {
            console.log('🚫 Data refresh debounced');
            return;
        }

        this.lastDataRefresh = now;
        await this.loadResearchSession();
        this.renderInterface();
    }

    // Simple event handlers - delegate all logic to backend
    setupEventListeners() {
        // Listen for judgment updates
        window.addEventListener('storage', (e) => {
            if (e.key === 'judgmentUpdate') {
                this.refreshData();
            }
        });

        // Listen for visibility changes
        document.addEventListener('visibilitychange', () => {
            if (!document.hidden) {
                this.refreshData();
            }
        });

        // Keyboard shortcut for manual refresh (Ctrl+Shift+R)
        document.addEventListener('keydown', (e) => {
            if (e.ctrlKey && e.shiftKey && e.key === 'R') {
                e.preventDefault();
                console.log('🔄 Manual refresh triggered');
                this.refreshData();
            }
        });
    }

    // Simple UI rendering - uses backend-provided data structures
    renderInterface() {
        try {
            // Hide loading state
            const loadingState = document.getElementById('loadingState');
            if (loadingState) {
                loadingState.style.display = 'none';
            }

            this.renderResearchSession();
            this.updatePatientCounts();
        } catch (error) {
            console.error('Failed to render interface:', error);
            // Show error message to user
            const container = document.getElementById('patientAccordion');
            if (container) {
                container.innerHTML = `
                    <div class="no-active-session">
                        <h3>Interface Error</h3>
                        <p>Failed to render the interface. Please refresh the page.</p>
                        <div class="creation-status error" style="display: block;">
                            Error: ${error.message || error}
                        </div>
                    </div>
                `;
            }
        }
    }

    // Render research session using backend data
    renderResearchSession() {
        const container = document.getElementById('patientAccordion');
        if (!container) {
            console.warn('Patient accordion container not found');
            return;
        }

        if (!this.sessionData || !this.sessionData.active_chunk) {
            container.innerHTML = `
                <div class="no-active-session">
                    <h3>No Active Research Session</h3>
                    <p>No research session is currently active and no cohorts are available for review.</p>

                    <div class="cohorts-help">
                        <h4>What You Need</h4>
                        <p>This environment expects a prepared clinical workspace database that already contains review cohorts and patient assignments. For local development, import or seed sample cohort data before retrying.</p>
                    </div>

                    <div class="cohort-guidance">
                        <h4>ETL-Driven Cohort Workflow</h4>
                        <p>This application now uses a database-centric approach for patient data management:</p>

                        <div class="workflow-steps">
                            <h5>Step 1: Data Import (ETL System)</h5>
                            <ul>
                                <li>Use the ETL import system to load patient data into the database</li>
                                <li>Patient data is processed and validated during import</li>
                                <li>Data is stored with proper tenant isolation and security</li>
                            </ul>

                            <h5>Step 2: Cohort Creation</h5>
                            <ul>
                                <li>Create research cohorts from imported patient data</li>
                                <li>Define patient selection criteria and review parameters</li>
                                <li>Assign reviewers and configure access permissions</li>
                            </ul>

                            <h5>Step 3: Review Session</h5>
                            <ul>
                                <li>Return to this page after cohorts are created</li>
                                <li>Select an available cohort to begin review</li>
                                <li>System will automatically create review chunks and track progress</li>
                            </ul>
                        </div>

                        <div class="migration-note">
                            <h5>Local workflow</h5>
                            <p>Imported data, cohorts, and review progress remain in the configured local SQLite database.</p>
                            <ul>
                                <li>Use only synthetic or properly authorized research data</li>
                                <li>Enable PII purging for documented imports</li>
                                <li>Back up and protect the database according to your research protocol</li>
                            </ul>
                        </div>
                    </div>
                </div>
            `;
            return;
        }

        const activeChunk = this.sessionData.active_chunk;
        const progress = activeChunk.completed_patients || 0;
        const total = activeChunk.total_patients || activeChunk.patient_ids?.length || 0;
        const progressPercent = total > 0 ? (progress / total) * 100 : 0;

        container.innerHTML = `
            <div class="accordion-section active-chunk" data-section="active">
                <div class="accordion-header active">
                    <h3>Current Work - Batch ${activeChunk.id}</h3>
                    <div class="chunk-progress">
                        <span class="progress-text">${progress} of ${total} completed</span>
                        <div class="progress-bar">
                            <div class="progress-fill" style="width: ${progressPercent}%"></div>
                        </div>
                    </div>
                    <span class="accordion-toggle">−</span>
                </div>
                <div class="accordion-content expanded">
                    <div class="patient-grid">
                        ${this.renderPatientCards(activeChunk.patient_ids || [])}
                    </div>
                    ${progress === total ? '<div class="chunk-complete-message">Batch completed. Next batch will activate automatically.</div>' : ''}
                </div>
            </div>
            ${this.renderCompletedChunks()}
            ${this.renderStatistics()}
        `;

        this.setupAccordionEvents();
    }

    // Render patient cards using cached patient data with judgment status
    renderPatientCards(patientIds) {
        if (!this.cachedPatients || !patientIds) return '';

        return patientIds.map(patientId => {
            const patient = this.cachedPatients.find(p => p.id === patientId);
            if (!patient) return '';

            // Determine judgment status and styling
            const judgmentClass = patient.has_judgment ? 'judged' : 'pending';
            const judgmentIndicator = patient.has_judgment ? 'Reviewed' : 'Pending';
            const judgmentColor = patient.has_judgment ? '#28a745' : '#ffc107';

            return `
                <a href="patient.html?id=${encodeURIComponent(patient.id)}" class="patient-card-link">
                    <div class="patient-card active-chunk-patient ${judgmentClass}">
                        <div class="patient-id">${patient.id}</div>
                        <div class="patient-info">
                            <span class="patient-age">${patient.age || 'N/A'}</span>
                            <span class="patient-sex">${patient.sex || 'N/A'}</span>
                        </div>
                        <div class="judgment-status" style="color: ${judgmentColor}; font-weight: bold; font-size: 12px;">
                            ${judgmentIndicator}
                        </div>
                        <div class="active-indicator">Current Work</div>
                    </div>
                </a>
            `;
        }).join('');
    }

    // Render completed chunks section
    renderCompletedChunks() {
        if (!this.sessionData.completed_chunks || this.sessionData.completed_chunks.length === 0) {
            return '';
        }

        return `
            <div class="accordion-section completed" data-section="completed">
                <div class="accordion-header">
                    <h3>Completed Batches (${this.sessionData.completed_chunks.length})</h3>
                    <span class="accordion-toggle">+</span>
                </div>
                <div class="accordion-content">
                    <div class="completed-chunks-summary">
                        ${this.sessionData.completed_chunks.map(chunk =>
                            `<div class="completed-chunk-item">Batch ${chunk} completed</div>`
                        ).join('')}
                    </div>
                </div>
            </div>
        `;
    }

    // Render statistics section
    renderStatistics() {
        return `
            <div class="accordion-section statistics" data-section="statistics">
                <div class="accordion-header">
                    <h3>Research Progress</h3>
                    <span class="accordion-toggle">+</span>
                </div>
                <div class="accordion-content">
                    <div class="statistics-grid">
                        <div class="stat-item">
                            <span class="stat-label">Total Patients</span>
                            <span class="stat-value" id="totalPatients">${this.sessionData.total_patients || 0}</span>
                        </div>
                        <div class="stat-item">
                            <span class="stat-label">Judged</span>
                            <span class="stat-value" id="judgedPatients">${this.sessionData.judged_patients || 0}</span>
                        </div>
                        <div class="stat-item">
                            <span class="stat-label">Remaining</span>
                            <span class="stat-value" id="pendingPatients">${(this.sessionData.total_patients || 0) - (this.sessionData.judged_patients || 0)}</span>
                        </div>
                    </div>
                </div>
            </div>
        `;
    }

    // Setup accordion expand/collapse
    setupAccordionEvents() {
        document.querySelectorAll('.accordion-header').forEach(header => {
            // Remove existing listeners to prevent duplicates
            header.replaceWith(header.cloneNode(true));
        });

        // Add fresh event listeners
        document.querySelectorAll('.accordion-header').forEach(header => {
            header.addEventListener('click', () => {
                const content = header.nextElementSibling;
                const toggle = header.querySelector('.accordion-toggle');
                const isExpanded = content.classList.contains('expanded');

                if (isExpanded) {
                    content.classList.remove('expanded');
                    toggle.textContent = '+';
                } else {
                    content.classList.add('expanded');
                    toggle.textContent = '−';
                }
            });
        });
    }

    // Update patient counts in header
    updatePatientCounts() {
        const totalElement = document.getElementById('totalPatients');
        const judgedElement = document.getElementById('judgedPatients');
        const pendingElement = document.getElementById('pendingPatients');

        if (this.sessionData) {
            const total = this.sessionData.total_patients || 0;
            const judged = this.sessionData.judged_patients || 0;
            const pending = total - judged;

            if (totalElement) totalElement.textContent = total;
            if (judgedElement) judgedElement.textContent = judged;
            if (pendingElement) pendingElement.textContent = pending;
        }
    }


    // Check if cohorts workflow should be used (optimized to avoid duplicate calls)
    async shouldUseCohortWorkflow(sessionData) {
        try {
            if (sessionData && sessionData.active_chunk) {
                // Active session exists, don't show cohorts
                console.log('✅ Active research session found, using session workflow');
                return false;
            }

            // No active session, check if cohorts are available
            console.log('🔍 No active session, checking for available cohorts...');
            const availableCohorts = await TauriAPI.getAvailableCohorts();
            console.log('🔍 Backend returned cohorts:', availableCohorts);

            const hasAvailableCohorts = availableCohorts && availableCohorts.length > 0;

            if (hasAvailableCohorts) {
                console.log(`✅ Found ${availableCohorts.length} available cohorts, using cohort workflow`);
                console.log('📋 Available cohorts:', availableCohorts.map(c => `${c.name} (${c.id})`));
                return true;
            } else {
                console.log('ℹ️ No cohorts available - this is expected if no cohorts have been created via ETL');
                console.log('💡 To see cohort selection: 1) Use ETL system to import patient data, 2) Create research cohorts, 3) Refresh this page');
                return false;
            }
        } catch (error) {
            throw new Error('Failed to determine the cohort workflow', { cause: error });
        }
    }

    // Initialize cohort manager instead of research session
    async initializeCohortManager() {
        try {
            if (window.cohortManager) {
                console.log('CohortManager already exists, refreshing...');
                await window.cohortManager.refreshCohortsData();
                return;
            }

            // Create cohort manager instance
            window.cohortManager = new window.CohortManager();
            await window.cohortManager.ready;
            console.log('✅ CohortManager initialized');
        } catch (error) {
            throw new Error('Failed to initialize the cohort manager', { cause: error });
        }
    }

    async forceProgressUpdate() {
        const sessionData = await this.getSessionData();
        if (await this.shouldUseCohortWorkflow(sessionData)) {
            await this.initializeCohortManager();
        } else {
            await this.refreshData();
        }
    }
}

// Initialize when app is ready
document.addEventListener('appInitialized', (event) => {
    console.log('AppManager initialized, starting ResearchChunkManager...');

    if (window.researchChunkManagerInitialized) {
        console.log('ResearchChunkManager already initialized, skipping');
        return;
    }

    window.researchChunkManagerInitialized = true;

    if (!window.researchChunkManager) {
        console.log('Creating simplified ResearchChunkManager instance');
        window.researchChunkManager = new ResearchChunkManager();
    }
});
