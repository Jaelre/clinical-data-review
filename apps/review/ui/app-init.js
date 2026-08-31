// Centralized app initialization and local operator session management.

class AppConfig {
    constructor() {
        this.ui = {
            judgment: {
                values: {},
                messages: {}
            },
            patientList: {
                errorMessage: 'Error loading patient data. Please check the local workspace database.',
                loadingMessage: 'Loading patients...',
                noDataMessage: 'No patient data found.'
            }
        };
        this.initialized = false;
        this.initialData = null;
    }

    async initialize() {
        if (this.initialized) {
            return;
        }

        if (!window.isTauriContext || !window.TauriAPI) {
            throw new Error('Application configuration requires the Clinical Data Review Tauri runtime');
        }

        const initialData = await TauriAPI.invoke('get_initial_data');
        if (!initialData?.judgment_values || !initialData?.ui_messages) {
            throw new Error('The backend returned incomplete initialization data');
        }

        this.initialData = initialData;
        this.ui.judgment.values = initialData.judgment_values;
        this.ui.judgment.messages = initialData.ui_messages;
        this.initialized = true;
    }
}

class OperatorSessionManager {
    constructor() {
        this.state = null;
        this.overlay = null;
        this.statusChip = null;
        this.switchButton = null;
    }

    async initialize() {
        this.state = await TauriAPI.getOperatorSessionState();
        this.ensureHeaderChrome();
        this.renderHeaderState();

        if (this.state.requires_operator_selection) {
            await this.openMask();
        }

        return this.state;
    }

    async refreshState() {
        this.state = await TauriAPI.getOperatorSessionState();
        this.renderHeaderState();
        return this.state;
    }

    ensureHeaderChrome() {
        const header = document.querySelector('header');
        if (!header) {
            return;
        }

        let chrome = header.querySelector('.operator-session-chrome');
        if (!chrome) {
            chrome = document.createElement('div');
            chrome.className = 'operator-session-chrome';
            chrome.innerHTML = `
                <div class="operator-session-badge" id="operatorSessionBadge"></div>
                <button type="button" class="operator-session-switch" id="operatorSessionSwitch">
                    Switch Operator
                </button>
            `;
            header.appendChild(chrome);
        }

        this.statusChip = chrome.querySelector('#operatorSessionBadge');
        this.switchButton = chrome.querySelector('#operatorSessionSwitch');
        this.switchButton.addEventListener('click', () => {
            this.openMask().catch(error => {
                console.error('Failed to reopen operator mask:', error);
                window.ToastNotification?.error?.('Failed to load local operators.');
            });
        });
    }

    renderHeaderState() {
        if (!this.statusChip || !this.state) {
            return;
        }

        const operatorLabel = this.state.operator
            ? `${this.state.operator.display_name} · ${this.state.workspace_name}`
            : `No operator selected · ${this.state.workspace_name}`;

        this.statusChip.textContent = `${this.state.database_backend} local workspace: ${operatorLabel}`;
    }

    async openMask() {
        const operators = await TauriAPI.listLocalOperators();
        const overlay = this.ensureOverlay();

        overlay.querySelector('.operator-mask-title').textContent = `Select a local operator for ${this.state.workspace_name}`;
        overlay.querySelector('.operator-mask-subtitle').textContent =
            'This machine keeps separate review sessions per operator. Password login is disabled.';

        const list = overlay.querySelector('.operator-mask-list');
        list.innerHTML = '';

        if (operators.length === 0) {
            const empty = document.createElement('div');
            empty.className = 'operator-mask-empty';
            empty.textContent = 'No operators exist yet. Create the first local operator to begin reviewing.';
            list.appendChild(empty);
        } else {
            operators.forEach(operator => {
                const button = document.createElement('button');
                button.type = 'button';
                button.className = 'operator-mask-option';
                button.innerHTML = `
                    <span class="operator-mask-option-name">${operator.display_name}</span>
                    <span class="operator-mask-option-meta">${operator.email}</span>
                `;
                button.addEventListener('click', async () => {
                    await this.activateOperator(operator.id);
                });
                list.appendChild(button);
            });
        }

        const form = overlay.querySelector('.operator-mask-form');
        const input = overlay.querySelector('#newLocalOperatorName');
        const error = overlay.querySelector('.operator-mask-error');
        error.textContent = '';
        input.value = '';

        form.onsubmit = async event => {
            event.preventDefault();
            const displayName = input.value.trim();
            if (!displayName) {
                error.textContent = 'Enter a name for the local operator.';
                return;
            }

            try {
                await this.createOperator(displayName);
            } catch (creationError) {
                error.textContent = creationError.message || String(creationError);
            }
        };

        overlay.style.display = 'flex';
        input.focus();
    }

    ensureOverlay() {
        if (this.overlay) {
            return this.overlay;
        }

        const overlay = document.createElement('div');
        overlay.className = 'operator-mask-overlay';
        overlay.innerHTML = `
            <div class="operator-mask-panel">
                <div class="operator-mask-eyebrow">Local Review Session</div>
                <h2 class="operator-mask-title"></h2>
                <p class="operator-mask-subtitle"></p>

                <div class="operator-mask-section">
                    <div class="operator-mask-section-title">Available Operators</div>
                    <div class="operator-mask-list"></div>
                </div>

                <div class="operator-mask-section">
                    <div class="operator-mask-section-title">Create Operator</div>
                    <form class="operator-mask-form">
                        <input
                            id="newLocalOperatorName"
                            class="operator-mask-input"
                            type="text"
                            placeholder="e.g. Example Reviewer"
                            autocomplete="off"
                        />
                        <button type="submit" class="operator-mask-create">Create and Start Session</button>
                    </form>
                    <div class="operator-mask-error"></div>
                </div>
            </div>
        `;

        document.body.appendChild(overlay);
        this.overlay = overlay;
        return overlay;
    }

    async activateOperator(operatorId) {
        this.state = await TauriAPI.selectLocalOperator(operatorId);
        this.closeMask();
        this.renderHeaderState();
        this.notifyReady();
        window.location.reload();
    }

    async createOperator(displayName) {
        this.state = await TauriAPI.createLocalOperator(displayName);
        this.closeMask();
        this.renderHeaderState();
        this.notifyReady();
        window.location.reload();
    }

    closeMask() {
        if (this.overlay) {
            this.overlay.style.display = 'none';
        }
    }

    notifyReady() {
        document.dispatchEvent(new CustomEvent('operatorSessionReady', {
            detail: { operatorSession: this.state }
        }));
    }
}

class ApplicationManager {
    constructor() {
        this.services = new Map();
        this.initialized = false;
        this.operatorSessionManager = new OperatorSessionManager();
    }

    async initialize() {
        if (this.initialized) {
            return;
        }

        try {
            await window.AppConfig.initialize();
            this.registerService('config', window.AppConfig);

            const operatorSession = await this.operatorSessionManager.initialize();
            this.registerService('operatorSession', operatorSession);

            const judgmentManager = new TauriJudgmentManager();
            this.registerService('judgmentManager', judgmentManager);

            const judgmentChecker = new TauriJudgmentChecker();
            this.registerService('judgmentChecker', judgmentChecker);

            this.initialized = true;
            document.dispatchEvent(new CustomEvent('appInitialized', {
                detail: {
                    services: Array.from(this.services.keys()),
                    manager: this
                }
            }));
        } catch (error) {
            console.error('Application initialization failed:', error);
            throw error;
        }
    }

    registerService(name, service) {
        this.services.set(name, service);
    }

    getService(name) {
        return this.services.get(name);
    }
}

window.AppConfig = new AppConfig();
window.AppManager = new ApplicationManager();

document.addEventListener('DOMContentLoaded', async () => {
    try {
        await window.AppManager.initialize();
    } catch (error) {
        console.error('Failed to initialize application:', error);
        window.ToastNotification?.error?.('Application initialization failed. Check the local workspace database.');
    }
});
