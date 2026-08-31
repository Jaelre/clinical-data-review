// Patient search and grouping controls.
// Performance-focused implementation with lightweight UI animations

class PatientSearchManager {
    constructor() {
        this.patients = [];
        this.filteredPatients = [];
        this.searchIndex = new Map();
        this.currentQuery = '';
        this.currentFilters = {};
        this.currentGroupBy = 'none';
        this.pageSize = 50;
        this.currentPage = 0;

        // Performance design decision: Using instant show/hide for groups instead of
        // smooth animations to avoid processor-heavy operations with large datasets.
        // If UI feels too abrupt, we can add minimal 50-100ms opacity transitions later.
        this.useMinimalAnimations = true;

        this.init();
    }

    async init() {
        this.setupEventListeners();

        // Wait for existing judgment system to load first
        await this.waitForJudgmentSystem();

        await this.loadPatients();
        this.buildSearchIndex();
        this.renderPatients();
    }

    async waitForJudgmentSystem() {
        // Wait for the existing judgment system to be available
        let attempts = 0;
        const maxAttempts = 20; // 2 seconds max wait - reduced to prevent hanging

        while (attempts < maxAttempts) {
            if (window.judgmentChecker && typeof window.judgmentChecker.loadJudgments === 'function') {
                // Load judgments if not already loaded
                try {
                    await window.judgmentChecker.loadJudgments();
                    console.log('Patient search: Judgment system loaded and ready');
                    return;
                } catch (error) {
                    console.warn('Patient search: Failed to load judgments:', error);
                    return; // Continue anyway
                }
            }

            await new Promise(resolve => setTimeout(resolve, 100));
            attempts++;
        }

        console.warn('Patient search: Judgment system not available, continuing without judgment integration');
    }

    setupEventListeners() {
        // Search input with debouncing
        const searchInput = document.getElementById('patientSearch');
        let searchTimeout;
        searchInput.addEventListener('input', (e) => {
            clearTimeout(searchTimeout);
            searchTimeout = setTimeout(() => {
                this.handleSearch(e.target.value);
            }, 300);
        });

        // Clear search
        document.getElementById('clearSearch').addEventListener('click', () => {
            this.clearSearch();
        });

        // Filter controls
        document.getElementById('judgmentFilter').addEventListener('change', (e) => {
            this.handleFilterChange('judgment', e.target.value);
        });

        document.getElementById('groupBy').addEventListener('change', (e) => {
            this.handleGroupChange(e.target.value);
        });

        // Advanced filters modal
        document.getElementById('advancedFilters').addEventListener('click', () => {
            this.showAdvancedFilters();
        });

        document.getElementById('closeAdvancedFilters').addEventListener('click', () => {
            this.hideAdvancedFilters();
        });

        document.getElementById('applyAdvancedFilters').addEventListener('click', () => {
            this.applyAdvancedFilters();
        });

        document.getElementById('resetAdvancedFilters').addEventListener('click', () => {
            this.resetAdvancedFilters();
        });

        // Clear from empty state
        document.getElementById('clearSearchFromEmpty').addEventListener('click', () => {
            this.clearSearch();
        });

        // Load more pagination
        document.getElementById('loadMoreBtn').addEventListener('click', () => {
            this.loadMorePatients();
        });

        // Modal overlay click to close
        document.getElementById('advancedFiltersModal').addEventListener('click', (e) => {
            if (e.target.id === 'advancedFiltersModal') {
                this.hideAdvancedFilters();
            }
        });
    }

    async loadPatients() {
        try {
            if (window.isTauriContext) {
                // Check for patient selection info
                const selectionInfo = await TauriAPI.getPatientSelectionInfo();
                this.displaySelectionStatus(selectionInfo);

                // Load patient data directly from backend
                this.patients = await TauriAPI.loadAllPatientData();
            } else {
                // Fallback for non-Tauri environments - direct backend call
                console.log('Loading patient data via direct backend call...');
                this.patients = await TauriAPI.loadAllPatientData();
            }

            this.filteredPatients = [...this.patients];
            this.updatePatientCount();
        } catch (error) {
            console.error('Failed to load patients:', error);
            this.showError('Failed to load patient data. Please try again.');
        }
    }

    buildSearchIndex() {
        this.searchIndex.clear();

        this.patients.forEach(patient => {
            const searchTerms = [
                patient.id,
                patient.age || '',
                patient.sex || '',
                ...(patient.past_history || []),
                ...(patient.medication || []),
                ...(patient.allergies || []),
                ...(patient.recent_history || [])
            ].join(' ').toLowerCase();

            this.searchIndex.set(patient.id, searchTerms);
        });
    }

    handleSearch(query) {
        this.currentQuery = query.toLowerCase().trim();
        this.filterAndRender();

        // Show/hide clear search button
        const clearBtn = document.getElementById('clearSearch');
        clearBtn.style.display = this.currentQuery ? 'block' : 'none';
    }

    clearSearch() {
        document.getElementById('patientSearch').value = '';
        this.currentQuery = '';
        document.getElementById('clearSearch').style.display = 'none';
        this.filterAndRender();
    }

    handleFilterChange(filterType, value) {
        if (value === 'all' || value === '') {
            delete this.currentFilters[filterType];
        } else {
            this.currentFilters[filterType] = value;
        }

        this.filterAndRender();
        this.updateClearFiltersButton();
    }

    handleGroupChange(groupBy) {
        this.currentGroupBy = groupBy;
        this.renderPatients();
    }

    filterAndRender() {
        this.currentPage = 0;
        this.applyFilters();
        this.renderPatients();
    }

    applyFilters() {
        this.filteredPatients = this.patients.filter(patient => {
            // Text search filter
            if (this.currentQuery) {
                const searchText = this.searchIndex.get(patient.id) || '';
                if (!searchText.includes(this.currentQuery)) {
                    return false;
                }
            }

            // Judgment filter
            if (this.currentFilters.judgment) {
                const hasJudgment = this.hasPatientJudgment(patient.id);
                const judgment = this.getPatientJudgment(patient.id);

                if (this.currentFilters.judgment === 'pending' && hasJudgment) {
                    return false;
                }
                if (this.currentFilters.judgment !== 'pending' && judgment !== this.currentFilters.judgment) {
                    return false;
                }
            }

            // Age range filter
            if (this.currentFilters.ageRange) {
                const [minAge, maxAge] = this.currentFilters.ageRange;
                const age = parseInt(patient.age) || 0;
                if (age < minAge || age > maxAge) {
                    return false;
                }
            }

            // Sex filter
            if (this.currentFilters.sex) {
                if (patient.sex !== this.currentFilters.sex) {
                    return false;
                }
            }

            // Allergies filter
            if (this.currentFilters.hasAllergies !== undefined) {
                const hasAllergies = patient.allergies && patient.allergies.length > 0;
                if (hasAllergies !== this.currentFilters.hasAllergies) {
                    return false;
                }
            }

            // Medications filter
            if (this.currentFilters.hasMedications !== undefined) {
                const hasMedications = patient.medication && patient.medication.length > 0;
                if (hasMedications !== this.currentFilters.hasMedications) {
                    return false;
                }
            }

            return true;
        });

        this.updatePatientCount();
    }

    async groupPatients() {
        if (this.currentGroupBy === 'none') {
            return { 'All Patients': this.filteredPatients };
        }

        // Use backend grouping for better performance with large datasets if available
        if (window.isTauriContext) {
            try {
                const groups = await TauriAPI.getPatientGroups(this.currentGroupBy);

                // Filter groups to only include patients in current filtered set
                const filteredPatientIds = new Set(this.filteredPatients.map(p => p.id));
                const filteredGroups = {};

                Object.entries(groups).forEach(([groupName, patientIds]) => {
                    const filteredIds = patientIds.filter(id => filteredPatientIds.has(id));
                    if (filteredIds.length > 0) {
                        filteredGroups[groupName] = this.filteredPatients.filter(p => filteredIds.includes(p.id));
                    }
                });

                return filteredGroups;
            } catch (error) {
                console.error('Failed to get patient groups, using client-side grouping:', error);
            }
        }

        // Fallback to client-side grouping for non-Tauri environments
        return this.groupPatientsClientSide();
    }

    groupPatientsClientSide() {
        const groups = {};

        this.filteredPatients.forEach(patient => {
            let groupKey;

            switch (this.currentGroupBy) {
                case 'judgment_status':
                    const hasJudgment = this.hasPatientJudgment(patient.id);
                    const judgment = this.getPatientJudgment(patient.id);
                    if (hasJudgment && judgment) {
                        groupKey = judgment === 'appropriate' ? 'Appropriate' : 'Not Appropriate';
                    } else {
                        groupKey = 'Pending Review';
                    }
                    break;

                case 'age_range':
                    const age = parseInt(patient.age) || 0;
                    if (age < 18) groupKey = 'Under 18';
                    else if (age < 30) groupKey = '18-29';
                    else if (age < 50) groupKey = '30-49';
                    else if (age < 70) groupKey = '50-69';
                    else groupKey = '70+';
                    break;

                case 'sex':
                    groupKey = patient.sex === 'M' ? 'Male' :
                              patient.sex === 'F' ? 'Female' : 'Unknown';
                    break;

                default:
                    groupKey = 'All Patients';
            }

            if (!groups[groupKey]) {
                groups[groupKey] = [];
            }
            groups[groupKey].push(patient);
        });

        return groups;
    }

    async renderPatients() {
        const groupsContainer = document.getElementById('patientGroups');
        const listContainer = document.getElementById('patientListContainer');
        const loadingState = document.getElementById('loadingState');
        const noResultsState = document.getElementById('noResultsState');

        console.log('renderPatients called with', this.filteredPatients.length, 'patients');
        console.log('DOM elements found:', {
            groupsContainer: !!groupsContainer,
            listContainer: !!listContainer,
            loadingState: !!loadingState,
            noResultsState: !!noResultsState
        });

        // Hide loading state
        if (loadingState) {
            loadingState.style.display = 'none';
            console.log('Loading state hidden');
        }

        // Check for no results
        if (this.filteredPatients.length === 0) {
            groupsContainer.style.display = 'none';
            listContainer.style.display = 'none';
            noResultsState.style.display = 'block';
            return;
        }

        noResultsState.style.display = 'none';

        const groups = await this.groupPatients();

        if (this.currentGroupBy === 'none') {
            // Render as simple list
            groupsContainer.style.display = 'none';
            listContainer.style.display = 'block';
            this.renderPatientList(this.filteredPatients);
        } else {
            // Render as grouped sections
            listContainer.style.display = 'none';
            groupsContainer.style.display = 'block';
            this.renderPatientGroups(groups);
        }

        this.updatePaginationControls();
    }

    renderPatientGroups(groups) {
        const container = document.getElementById('patientGroups');
        container.innerHTML = '';

        Object.entries(groups).forEach(([groupName, patients]) => {
            const groupElement = this.createGroupElement(groupName, patients);
            container.appendChild(groupElement);
        });
    }

    createGroupElement(groupName, patients) {
        const groupDiv = document.createElement('div');
        groupDiv.className = 'patient-group';

        const isCollapsed = localStorage.getItem(`group_${groupName}_collapsed`) === 'true';

        groupDiv.innerHTML = `
            <div class="group-header" data-group="${groupName}">
                <h3>
                    ${groupName}
                    <span class="group-count">(${patients.length})</span>
                    <span class="group-toggle ${isCollapsed ? 'collapsed' : ''}">${isCollapsed ? 'Show' : 'Hide'}</span>
                </h3>
            </div>
            <div class="group-content ${isCollapsed ? 'collapsed' : ''}">
                <div class="patient-grid">
                    ${this.renderPatientCards(patients)}
                </div>
            </div>
        `;

        // Add click handler for group toggle
        const header = groupDiv.querySelector('.group-header');
        header.addEventListener('click', () => {
            this.toggleGroup(groupName);
        });

        return groupDiv;
    }

    renderPatientList(patients) {
        const container = document.getElementById('patientList');
        const paginatedPatients = patients.slice(0, (this.currentPage + 1) * this.pageSize);
        container.innerHTML = this.renderPatientCards(paginatedPatients);
    }

    renderPatientCards(patients) {
        return patients.map(patient => {
            const judgment = this.getPatientJudgment(patient.id);
            const judgmentClass = judgment ? `judgment-${judgment}` : 'judgment-pending';
            const judgmentLabel = judgment === 'appropriate' ? 'A' :
                                 judgment === 'not_appropriate' ? 'N' : 'P';

            return `
                <a href="patient.html?id=${encodeURIComponent(patient.id)}" class="patient-item ${judgmentClass}">
                    <div class="patient-header">
                        <strong>Patient ID: ${patient.id}</strong>
                    </div>
                    <div class="patient-meta">
                        Age: ${patient.age || 'Unknown'}, Sex: ${patient.sex || 'Unknown'}
                    </div>
                    <div class="judgment-indicator">
                        <span class="judgment-icon">${judgmentLabel}</span>
                    </div>
                </a>
            `;
        }).join('');
    }

    toggleGroup(groupName) {
        const groupElement = document.querySelector(`[data-group="${groupName}"]`).parentElement;
        const content = groupElement.querySelector('.group-content');
        const toggle = groupElement.querySelector('.group-toggle');

        const isCollapsed = content.classList.contains('collapsed');

        // Performance design decision: Using instant display toggle instead of smooth height animation
        // to avoid layout recalculation with large patient lists
        if (isCollapsed) {
            content.classList.remove('collapsed');
            toggle.classList.remove('collapsed');
            toggle.textContent = 'Hide';
            localStorage.setItem(`group_${groupName}_collapsed`, 'false');
        } else {
            content.classList.add('collapsed');
            toggle.classList.add('collapsed');
            toggle.textContent = 'Show';
            localStorage.setItem(`group_${groupName}_collapsed`, 'true');
        }
    }

    loadMorePatients() {
        this.currentPage++;
        this.renderPatients();
    }

    updatePatientCount() {
        const countElement = document.getElementById('patientCount');
        const total = this.patients.length;
        const filtered = this.filteredPatients.length;

        if (filtered === total) {
            countElement.textContent = `Showing ${total} patients`;
        } else {
            countElement.textContent = `Showing ${filtered} of ${total} patients`;
        }
    }

    updatePaginationControls() {
        const controls = document.getElementById('paginationControls');
        const loadMoreBtn = document.getElementById('loadMoreBtn');
        const pageInfo = document.getElementById('pageInfo');

        const totalShown = Math.min((this.currentPage + 1) * this.pageSize, this.filteredPatients.length);
        const hasMore = totalShown < this.filteredPatients.length;

        if (this.currentGroupBy === 'none' && hasMore) {
            controls.style.display = 'block';
            pageInfo.textContent = `Showing ${totalShown} of ${this.filteredPatients.length} patients`;
        } else {
            controls.style.display = 'none';
        }
    }

    updateClearFiltersButton() {
        const clearBtn = document.getElementById('clearFilters');
        const hasFilters = Object.keys(this.currentFilters).length > 0;
        clearBtn.style.display = hasFilters ? 'block' : 'none';
    }

    displaySelectionStatus(selectionInfo) {
        const statusElement = document.getElementById('selectionStatus');
        const descriptionElement = document.getElementById('selectionDescription');

        if (selectionInfo.is_filtered) {
            statusElement.style.display = 'block';
            descriptionElement.textContent =
                `Showing ${selectionInfo.description || 'selected patients'} (${selectionInfo.selected_count} of ${selectionInfo.total_available} total)`;

            // Add event listener for "View All" button
            document.getElementById('viewAllPatients').addEventListener('click', () => {
                // This would trigger loading all patients instead of selected subset
                console.log('View all patients requested');
            });
        } else {
            statusElement.style.display = 'none';
        }
    }

    showAdvancedFilters() {
        document.getElementById('advancedFiltersModal').style.display = 'block';
    }

    hideAdvancedFilters() {
        document.getElementById('advancedFiltersModal').style.display = 'none';
    }

    applyAdvancedFilters() {
        const ageMin = document.getElementById('ageMin').value;
        const ageMax = document.getElementById('ageMax').value;
        const sex = document.getElementById('sexFilter').value;
        const hasAllergies = document.getElementById('hasAllergies').checked;
        const hasMedications = document.getElementById('hasMedications').checked;

        // Apply age range filter
        if (ageMin || ageMax) {
            const min = parseInt(ageMin) || 0;
            const max = parseInt(ageMax) || 120;
            this.currentFilters.ageRange = [min, max];
        } else {
            delete this.currentFilters.ageRange;
        }

        // Apply sex filter
        if (sex) {
            this.currentFilters.sex = sex;
        } else {
            delete this.currentFilters.sex;
        }

        // Apply checkbox filters
        if (document.getElementById('hasAllergies').indeterminate === false) {
            this.currentFilters.hasAllergies = hasAllergies;
        } else {
            delete this.currentFilters.hasAllergies;
        }

        if (document.getElementById('hasMedications').indeterminate === false) {
            this.currentFilters.hasMedications = hasMedications;
        } else {
            delete this.currentFilters.hasMedications;
        }

        this.filterAndRender();
        this.updateClearFiltersButton();
        this.hideAdvancedFilters();
    }

    resetAdvancedFilters() {
        document.getElementById('ageMin').value = '';
        document.getElementById('ageMax').value = '';
        document.getElementById('sexFilter').value = '';
        document.getElementById('hasAllergies').checked = false;
        document.getElementById('hasMedications').checked = false;

        delete this.currentFilters.ageRange;
        delete this.currentFilters.sex;
        delete this.currentFilters.hasAllergies;
        delete this.currentFilters.hasMedications;

        this.filterAndRender();
        this.updateClearFiltersButton();
    }

    getPatientJudgment(patientId) {
        // Integrate with existing judgment system
        if (window.judgmentChecker && typeof window.judgmentChecker.getJudgment === 'function') {
            const judgment = window.judgmentChecker.getJudgment(patientId);
            if (judgment) {
                // Handle both string format and object format
                return typeof judgment === 'string' ? judgment : judgment.judgment;
            }
        }
        return null;
    }

    hasPatientJudgment(patientId) {
        // Check if patient has any judgment
        if (window.judgmentChecker && typeof window.judgmentChecker.hasJudgment === 'function') {
            return window.judgmentChecker.hasJudgment(patientId);
        }
        return false;
    }

    showError(message) {
        console.error(message);
        // Could show a toast notification or error state
    }
}

document.addEventListener('appInitialized', () => {
    if (!window.patientSearchManager) {
        window.patientSearchManager = new PatientSearchManager();
    }
});
