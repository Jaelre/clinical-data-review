class FeatureFlagsManager {
    constructor() {
        this.enabledFeatures = new Set();
        this.initialized = false;
    }

    async initialize() {
        const features = await this.loadFeatureFlags();
        this.enabledFeatures = new Set(features);
        this.applyFeatureUI();
        this.initialized = true;
        return true;
    }

    async loadFeatureFlags() {
        if (!window.isTauriContext || !window.TauriAPI) {
            throw new Error('Feature flags require the Clinical Data Review Tauri runtime');
        }
        return window.TauriAPI.invoke('get_feature_flags');
    }

    applyFeatureUI() {
        const body = document.body;
        for (const className of Array.from(body.classList)) {
            if (className.startsWith('feature-')) {
                body.classList.remove(className);
            }
        }
        for (const feature of this.enabledFeatures) {
            body.classList.add(`feature-${feature.replaceAll('_', '-')}`);
        }

        for (const element of document.querySelectorAll('[data-feature]')) {
            this.setElementVisible(
                element,
                this.enabledFeatures.has(element.getAttribute('data-feature'))
            );
        }
        for (const element of document.querySelectorAll('[data-feature-disabled]')) {
            this.setElementVisible(
                element,
                !this.enabledFeatures.has(element.getAttribute('data-feature-disabled'))
            );
        }
    }

    setElementVisible(element, visible) {
        element.style.display = visible ? '' : 'none';
        element.toggleAttribute('hidden', !visible);
    }
}

window.FeatureFlagsManager = FeatureFlagsManager;
window.featureFlagsManager = new FeatureFlagsManager();

async function initializeFeatureFlags() {
    try {
        await window.featureFlagsManager.initialize();
    } catch (error) {
        document.body.dataset.featureInitialization = 'failed';
        console.error('Feature flag initialization failed:', error);
    }
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initializeFeatureFlags, { once: true });
} else {
    initializeFeatureFlags();
}
