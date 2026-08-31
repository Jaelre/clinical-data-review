// Tauri API utilities for the local Clinical Data Review application.
const isTauriContext = typeof window?.__TAURI__?.core?.invoke === 'function';

window.isTauriContext = isTauriContext;

class TauriAPI {
    static normalizePatientId(patientOrId) {
        if (typeof patientOrId === 'string') {
            if (patientOrId === '[object Object]') {
                throw new Error('Invalid patient identifier: received stringified object token');
            }
            return patientOrId;
        }

        if (typeof patientOrId === 'number') {
            return String(patientOrId);
        }

        if (patientOrId && typeof patientOrId === 'object') {
            const candidateId =
                patientOrId.id ??
                patientOrId.patient_id ??
                patientOrId.patientId ??
                patientOrId.external_id ??
                patientOrId.externalId;

            if (typeof candidateId === 'string' || typeof candidateId === 'number') {
                return String(candidateId);
            }
        }

        throw new Error(`Invalid patient identifier: ${String(patientOrId)}`);
    }

    static async invoke(command, args = {}) {
        const invokeFunction = window.__TAURI__?.core?.invoke;
        if (!invokeFunction) {
            throw new Error('Tauri API not available');
        }
        return await invokeFunction(command, args);
    }

    static async getPatientJudgment(patient_id) {
        const patientId = this.normalizePatientId(patient_id);
        return await this.invoke('get_patient_judgment', { patientId });
    }

    static async savePatientJudgment(patient_id, judgment) {
        const patientId = this.normalizePatientId(patient_id);
        return await this.invoke('save_patient_judgment_with_chunk_detection', {
            patientId,
            judgment
        });
    }

    // Get judgment summary - delegates to backend service
    static async getJudgmentSummary() {
        return await this.invoke('get_judgment_summary');
    }

    static async getPatientSelectionInfo() {
        return await this.invoke('get_patient_selection_info');
    }

    static async getPatientGroups(groupBy) {
        return await this.invoke('get_patient_groups', { groupBy });
    }

    static async isFeatureEnabled(featureName) {
        return await this.invoke('is_feature_enabled', { featureName });
    }

    static async loadAllPatientData() {
        return await this.invoke('load_all_patient_data');
    }

    // Load patients by their external IDs - for research session chunks
    static async loadPatientsByIds(patientIds) {
        return await this.invoke('load_patients_by_ids', { patientIds: patientIds });
    }

    static async getPatientDetails(patientId) {
        const normalizedPatientId = this.normalizePatientId(patientId);
        return await this.invoke('get_patient_details', { patientId: normalizedPatientId });
    }

    static async getNextUnjudgedPatient(currentPatientId) {
        const normalizedPatientId = this.normalizePatientId(currentPatientId);
        const response = await this.invoke('get_next_unjudged_patient', {
            currentPatientId: normalizedPatientId
        });

        return response?.next_patient_id ?? null;
    }

    static async getOperatorSessionState() {
        return await this.invoke('get_operator_session_state');
    }

    static async listLocalOperators() {
        return await this.invoke('list_local_operators');
    }

    static async selectLocalOperator(operatorId) {
        return await this.invoke('select_local_operator', { operatorId });
    }

    static async createLocalOperator(displayName) {
        return await this.invoke('create_local_operator', { displayName });
    }

    static async flagForAdminReview(patient_id, reviewReason, flagType = 'admin_review') {
        const patientId = this.normalizePatientId(patient_id);
        return await this.invoke('flag_for_admin_review', {
            patientId,
            reason: reviewReason,
            flagType
        });
    }

    static async getAdminReviewStatus(patient_id) {
        const patientId = this.normalizePatientId(patient_id);
        return await this.invoke('get_admin_review_status', { patientId });
    }

    static async clearAdminReviewFlag(patient_id, resolutionNotes = null) {
        const patientId = this.normalizePatientId(patient_id);
        return await this.invoke('clear_admin_review_flag', {
            patientId,
            resolutionNotes
        });
    }

    static async getAvailableCohorts() {
        return await this.invoke('get_available_cohorts');
    }

    static async startReviewSessionForCohort(cohortId, sessionName) {
        return await this.invoke('start_review_session_for_cohort', {
            cohortId: String(cohortId),
            sessionName
        });
    }

    static async getResearchSession(sessionId = null) {
        return await this.invoke('get_research_session', { sessionId });
    }
}

class TauriJudgmentManager {
    constructor() {
        this.currentJudgment = null;
    }

    async loadExistingJudgment(patient_id) {
        const judgmentRecord = await TauriAPI.getPatientJudgment(patient_id);
        if (!judgmentRecord) {
            this.currentJudgment = null;
            return null;
        }

        this.currentJudgment = judgmentRecord.judgment;
        return judgmentRecord.judgment;
    }

    // Save judgment - delegates to backend with chunk detection
    async saveJudgment(patient_id, judgment) {
        try {
            console.log(`🚀 Saving judgment for patient ${patient_id}: ${judgment}`);

            // Use enhanced backend service that handles chunk detection automatically
            const chunkProgressed = await TauriAPI.savePatientJudgment(patient_id, judgment);
            this.currentJudgment = judgment;

            if (chunkProgressed) {
                console.log(`🎯 Judgment saved and research chunk progressed for patient ${patient_id}`);
            } else {
                console.log(`✅ Judgment saved for patient ${patient_id}`);
            }

            // Return success object with judgment details
            return {
                success: true,
                judgment: judgment,
                chunkProgressed: chunkProgressed,
                patientId: patient_id
            };
        } catch (error) {
            console.error(`❌ Failed to save judgment for patient ${patient_id}:`, error);
            // Return error object with details
            return {
                success: false,
                error: error.message || 'Unknown error occurred',
                judgment: null,
                chunkProgressed: false,
                patientId: patient_id
            };
        }
    }

    // Simple getters - no business logic
    getCurrentJudgment() {
        return this.currentJudgment;
    }

    setCurrentJudgment(judgment) {
        this.currentJudgment = judgment;
        this.updateJudgmentButtonsUI(judgment);
    }

    // Clear cached judgment when needed
    clearJudgment() {
        this.currentJudgment = null;
        this.updateJudgmentButtonsUI(null);
    }

    // Update judgment button UI to reflect current state
    updateJudgmentButtonsUI(judgment) {
        const appropriateBtn = document.getElementById('appropriateBtn');
        const notAppropriateBtn = document.getElementById('notAppropriateBtn');

        if (!appropriateBtn || !notAppropriateBtn) {
            console.warn('Judgment buttons not found in DOM');
            return;
        }

        // Reset both buttons to default state (remove selected class)
        appropriateBtn.classList.remove('selected');
        notAppropriateBtn.classList.remove('selected');

        // Apply selected state using existing CSS classes
        if (judgment) {
            // Handle both A/N format and appropriate/not_appropriate format
            const isAppropriate = judgment === 'A' ||
                                judgment === 'appropriate' ||
                                judgment === 'Accepted' ||
                                judgment === (AppConfig?.ui?.judgment?.values?.appropriate || 'appropriate');
            const selectedBtn = isAppropriate ? appropriateBtn : notAppropriateBtn;

            // Use existing CSS class that handles styling
            selectedBtn.classList.add('selected');

            console.log(`✅ Updated judgment button UI - ${judgment} button selected (isAppropriate: ${isAppropriate})`);
        } else {
            console.log('🔄 Reset judgment button UI - no judgment selected');
        }

        // Update any "no judgments recorded" messages
        this.updateJudgmentStatusMessage(judgment);
    }

    // Update judgment status message
    updateJudgmentStatusMessage(judgment) {
        const statusElement = document.getElementById('judgmentStatus');
        if (statusElement) {
            if (judgment) {
                // Convert A/N codes to human-readable format that matches UI buttons
                let displayText = judgment;
                if (judgment === 'A') displayText = 'Appropriate';
                else if (judgment === 'N') displayText = 'Not Appropriate';
                else if (judgment === 'appropriate') displayText = 'Appropriate';
                else if (judgment === 'not_appropriate') displayText = 'Not Appropriate';

                statusElement.textContent = `Judgment recorded: ${displayText}`;
                statusElement.style.color = 'hsl(var(--success))';
                statusElement.style.fontWeight = 'bold';
                statusElement.style.background = 'hsl(var(--success) / 0.1)';
            } else {
                statusElement.textContent = 'No judgment recorded';
                statusElement.style.color = 'hsl(var(--muted-foreground))';
                statusElement.style.fontWeight = 'normal';
                statusElement.style.background = 'hsl(var(--muted))';
            }
            console.log(`📝 Updated judgment status message: "${statusElement.textContent}"`);
        }
    }
}

class TauriJudgmentChecker {
    constructor() {
        this.cachedSummary = null;
        this.lastLoadTime = null;
    }

    async loadJudgments() {
        const summary = await TauriAPI.getJudgmentSummary();
        this.cachedSummary = summary;
        this.lastLoadTime = Date.now();
        return summary;
    }

    // Simple cache-based getters - no business logic
    hasJudgment(patient_id) {
        if (!this.cachedSummary?.recent_judgments) return false;
        return this.cachedSummary.recent_judgments.some(j => j.patient_id === patient_id);
    }

    getJudgment(patient_id) {
        if (!this.cachedSummary?.recent_judgments) return null;
        return this.cachedSummary.recent_judgments.find(j => j.patient_id === patient_id);
    }

    getAllJudgments() {
        return this.cachedSummary?.recent_judgments || [];
    }

    // Return cached summary data - backend handles all calculations
    getJudgmentBreakdown() {
        if (!this.cachedSummary) return { appropriate: 0, notAppropriate: 0, total: 0 };

        return {
            appropriate: this.cachedSummary.accepted_count || 0,
            notAppropriate: this.cachedSummary.needs_review_count || 0,
            uncertain: this.cachedSummary.uncertain_count || 0,
            total: this.cachedSummary.total_judgments || 0
        };
    }

    getRecentActivity() {
        // Return recent judgments from cached summary - backend handles sorting
        return this.cachedSummary?.recent_judgments?.slice(0, 10) || [];
    }

    // Clear cache when needed
    clearCache() {
        this.cachedSummary = null;
        this.lastLoadTime = null;
    }
}

// Global toast notification system.
class ToastNotification {
    static show(message, type = 'info', duration = 3000) {
        const toast = document.createElement('div');

        // Define colors for different toast types
        const colors = {
            success: 'hsl(var(--success))',
            error: 'hsl(var(--destructive))',
            warning: 'hsl(var(--warning))',
            info: 'hsl(var(--info))'
        };

        const textColors = {
            success: 'white',
            error: 'white',
            warning: 'white',
            info: 'hsl(var(--foreground))'
        };

        toast.style.cssText = `
            position: fixed;
            top: 20px;
            right: 20px;
            background: ${colors[type] || colors.info};
            color: ${textColors[type] || textColors.info};
            padding: 1rem 1.5rem;
            border-radius: var(--radius);
            box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
            z-index: 1001;
            animation: slideIn 0.3s ease-out;
            max-width: 300px;
            word-wrap: break-word;
        `;

        toast.textContent = message;
        document.body.appendChild(toast);

        // Auto-remove after specified duration
        setTimeout(() => {
            if (toast.parentElement) {
                toast.remove();
            }
        }, duration);
    }

    static success(message, duration = 3000) {
        this.show(message, 'success', duration);
    }

    static error(message, duration = 4000) {
        this.show(message, 'error', duration);
    }

    static warning(message, duration = 3500) {
        this.show(message, 'warning', duration);
    }

    static info(message, duration = 3000) {
        this.show(message, 'info', duration);
    }
}

window.TauriAPI = TauriAPI;
window.TauriJudgmentManager = TauriJudgmentManager;
window.TauriJudgmentChecker = TauriJudgmentChecker;
window.ToastNotification = ToastNotification;
window.isTauriContext = isTauriContext;
