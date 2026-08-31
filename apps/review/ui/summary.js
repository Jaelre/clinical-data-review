// All data operations now use the Tauri backend and the local workspace database.

// Helper function to format judgment text for display
function formatJudgmentText(judgment) {
    if (!judgment) return 'Unknown';

    // Convert snake_case and camelCase to proper case
    const formatted = judgment
        .replace(/_/g, ' ')                    // Replace underscores with spaces
        .replace(/([a-z])([A-Z])/g, '$1 $2')   // Add space before capital letters
        .toLowerCase()                         // Convert to lowercase
        .replace(/\b\w/g, l => l.toUpperCase()); // Capitalize first letter of each word

    return formatted;
}

// Pure Tauri environment - direct backend calls exclusively
const judgmentChecker = new TauriJudgmentChecker();
let summaryRefreshInFlight = null;

async function loadSummaryData() {
    try {
        const [sessionData, summary] = await Promise.all([
            TauriAPI.getResearchSession(null),
            judgmentChecker.loadJudgments()
        ]);

        const totalPatients = Number(sessionData?.total_patients || 0);
        const completedCount = Number(
            sessionData?.judged_patients ?? summary?.total_judgments ?? 0
        );
        const pendingCount = totalPatients - completedCount;
        const completionPercentage = totalPatients > 0 ? Math.round((completedCount / totalPatients) * 100) : 0;

        // Update summary cards
        document.getElementById('totalPatients').textContent = totalPatients.toLocaleString();
        document.getElementById('completedJudgments').textContent = completedCount.toLocaleString();
        document.getElementById('pendingJudgments').textContent = pendingCount.toLocaleString();
        document.getElementById('completionPercentage').textContent = `${completionPercentage}%`;

        // Update progress bar
        const progressFill = document.getElementById('progressFill');
        const progressText = document.getElementById('progressText');
        progressFill.style.width = `${completionPercentage}%`;
        progressText.textContent = `${completedCount} of ${totalPatients} patients reviewed (${completionPercentage}%)`;

        // Update judgment breakdown
        displayJudgmentBreakdown();

        // Update recent activity
        displayRecentActivity();

    } catch (error) {
        console.error('Error:', error);
        displayError('Error loading summary data. Please check console for details.');
    }
}

function refreshSummaryData() {
    if (!summaryRefreshInFlight) {
        summaryRefreshInFlight = loadSummaryData().finally(() => {
            summaryRefreshInFlight = null;
        });
    }

    return summaryRefreshInFlight;
}

function displayJudgmentBreakdown() {
    const breakdown = judgmentChecker.getJudgmentBreakdown();
    const container = document.getElementById('judgmentBreakdown');

    if (breakdown.total === 0) {
        container.innerHTML = '<div class="empty-state">No judgments recorded yet</div>';
        return;
    }

    let html = '<div class="breakdown-items">';

    // Process breakdown from TauriJudgmentChecker format
    const judgmentTypes = [
        { key: 'appropriate', label: 'Appropriate', count: breakdown.appropriate, colorClass: 'success' },
        { key: 'notAppropriate', label: 'Not Appropriate', count: breakdown.notAppropriate, colorClass: 'destructive' },
        { key: 'uncertain', label: 'Uncertain', count: breakdown.uncertain, colorClass: 'primary' }
    ];

    for (const judgmentType of judgmentTypes) {
        if (judgmentType.count > 0) {
            const percentage = Math.round((judgmentType.count / breakdown.total) * 100);

            html += `
                <div class="breakdown-item">
                    <div class="breakdown-label">
                        <span class="breakdown-dot ${judgmentType.colorClass}"></span>
                        ${judgmentType.label}
                    </div>
                    <div class="breakdown-stats">
                        <span class="breakdown-count">${judgmentType.count}</span>
                        <span class="breakdown-percentage">(${percentage}%)</span>
                    </div>
                </div>
            `;
        }
    }

    html += '</div>';
    container.innerHTML = html;
}

function displayRecentActivity() {
    const recentActivity = judgmentChecker.getRecentActivity();
    const container = document.getElementById('recentActivity');

    if (recentActivity.length === 0) {
        container.innerHTML = '<div class="empty-state">No recent activity</div>';
        return;
    }

    let html = '<div class="activity-items">';

    for (const activity of recentActivity) {
        const date = new Date(activity.timestamp);
        const timeAgo = getTimeAgo(date);
        const colorClass = activity.judgment.toLowerCase().includes('appropriate') ? 'success' :
                          activity.judgment.toLowerCase().includes('not') ? 'destructive' : 'primary';

        html += `
            <div class="activity-item">
                <div class="activity-dot ${colorClass}"></div>
                <div class="activity-content">
                    <div class="activity-main">
                        Patient <strong>${activity.patient_id || activity.patientId || 'Unknown'}</strong> marked as
                        <span class="activity-judgment ${colorClass}">${formatJudgmentText(activity.judgment)}</span>
                    </div>
                    <div class="activity-time">${timeAgo}</div>
                </div>
            </div>
        `;
    }

    html += '</div>';
    container.innerHTML = html;
}

function getTimeAgo(date) {
    const now = new Date();
    const diffMs = now - date;
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'Just now';
    if (diffMins < 60) return `${diffMins} minute${diffMins > 1 ? 's' : ''} ago`;
    if (diffHours < 24) return `${diffHours} hour${diffHours > 1 ? 's' : ''} ago`;
    return `${diffDays} day${diffDays > 1 ? 's' : ''} ago`;
}

function displayError(message) {
    const containers = ['judgmentBreakdown', 'recentActivity'];
    containers.forEach(id => {
        const element = document.getElementById(id);
        if (element) {
            element.innerHTML = `<div class="error">${message}</div>`;
        }
    });
}

document.addEventListener('appInitialized', () => {
    refreshSummaryData().catch(error => {
        console.error('Failed to initialize summary page:', error);
        displayError('Error loading summary data. Please check console for details.');
    });
});

window.addEventListener('storage', (event) => {
    if (event.key === 'judgmentUpdate') {
        refreshSummaryData().catch(error => {
            console.error('Failed to refresh summary after judgment update:', error);
        });
    }
});

window.addEventListener('patientJudgmentUpdated', () => {
    refreshSummaryData().catch(error => {
        console.error('Failed to refresh summary after patient judgment update:', error);
    });
});

document.addEventListener('visibilitychange', () => {
    if (!document.hidden) {
        refreshSummaryData().catch(error => {
            console.error('Failed to refresh summary after tab visibility change:', error);
        });
    }
});
