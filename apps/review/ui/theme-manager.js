class ThemeManager {
    constructor() {
        this.currentTheme = null;
        this.initialized = false;
    }

    async initialize() {
        if (!window.isTauriContext || !window.TauriAPI) {
            throw new Error('Theme initialization requires the Clinical Data Review Tauri runtime');
        }

        const theme = await window.TauriAPI.invoke('get_ui_theme');
        if (!theme || typeof theme.theme !== 'string' || theme.theme.trim() === '') {
            throw new Error('The backend returned an invalid UI theme');
        }

        this.currentTheme = theme;
        this.applyTheme();
        this.initialized = true;
        return theme;
    }

    applyTheme() {
        for (const className of Array.from(document.body.classList)) {
            if (className.startsWith('theme-')) {
                document.body.classList.remove(className);
            }
        }

        document.body.classList.add(`theme-${this.currentTheme.theme}`);
        document.documentElement.style.colorScheme = this.currentTheme.dark_mode ? 'dark' : 'light';
    }
}

window.ThemeManager = ThemeManager;
window.themeManager = new ThemeManager();

async function initializeTheme() {
    try {
        await window.themeManager.initialize();
    } catch (error) {
        document.body.dataset.themeInitialization = 'failed';
        console.error('Theme initialization failed:', error);
    }
}

if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initializeTheme, { once: true });
} else {
    initializeTheme();
}
