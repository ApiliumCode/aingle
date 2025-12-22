/**
 * Real-time Notification System
 * Toast notifications, sound alerts, and badge counters
 */

class NotificationManager {
    constructor() {
        this.notifications = [];
        this.maxNotifications = 5;
        this.soundEnabled = false;
        this.badgeCount = 0;
        this.container = null;
        this.sounds = {};

        this.init();
    }

    init() {
        this.createContainer();
        this.loadSounds();
        this.setupControls();
        this.restoreSettings();
    }

    createContainer() {
        // Create notification container if it doesn't exist
        let container = document.getElementById('notification-container');
        if (!container) {
            container = document.createElement('div');
            container.id = 'notification-container';
            container.className = 'notification-container';
            document.body.appendChild(container);
        }
        this.container = container;

        // Create badge if it doesn't exist
        let badge = document.getElementById('notification-badge');
        if (!badge) {
            badge = document.createElement('div');
            badge.id = 'notification-badge';
            badge.className = 'notification-badge hidden';
            badge.textContent = '0';
            document.body.appendChild(badge);
        }
        this.badge = badge;
    }

    loadSounds() {
        // Load notification sounds using Web Audio API
        this.audioContext = new (window.AudioContext || window.webkitAudioContext)();

        // Simple beep sound generator
        this.sounds.beep = () => {
            const oscillator = this.audioContext.createOscillator();
            const gainNode = this.audioContext.createGain();

            oscillator.connect(gainNode);
            gainNode.connect(this.audioContext.destination);

            oscillator.frequency.value = 800;
            oscillator.type = 'sine';

            gainNode.gain.setValueAtTime(0.3, this.audioContext.currentTime);
            gainNode.gain.exponentialRampToValueAtTime(0.01, this.audioContext.currentTime + 0.2);

            oscillator.start(this.audioContext.currentTime);
            oscillator.stop(this.audioContext.currentTime + 0.2);
        };

        this.sounds.success = () => {
            const oscillator = this.audioContext.createOscillator();
            const gainNode = this.audioContext.createGain();

            oscillator.connect(gainNode);
            gainNode.connect(this.audioContext.destination);

            oscillator.frequency.setValueAtTime(600, this.audioContext.currentTime);
            oscillator.frequency.setValueAtTime(800, this.audioContext.currentTime + 0.1);
            oscillator.type = 'sine';

            gainNode.gain.setValueAtTime(0.2, this.audioContext.currentTime);
            gainNode.gain.exponentialRampToValueAtTime(0.01, this.audioContext.currentTime + 0.2);

            oscillator.start(this.audioContext.currentTime);
            oscillator.stop(this.audioContext.currentTime + 0.2);
        };

        this.sounds.error = () => {
            const oscillator = this.audioContext.createOscillator();
            const gainNode = this.audioContext.createGain();

            oscillator.connect(gainNode);
            gainNode.connect(this.audioContext.destination);

            oscillator.frequency.setValueAtTime(400, this.audioContext.currentTime);
            oscillator.frequency.setValueAtTime(200, this.audioContext.currentTime + 0.15);
            oscillator.type = 'square';

            gainNode.gain.setValueAtTime(0.2, this.audioContext.currentTime);
            gainNode.gain.exponentialRampToValueAtTime(0.01, this.audioContext.currentTime + 0.2);

            oscillator.start(this.audioContext.currentTime);
            oscillator.stop(this.audioContext.currentTime + 0.2);
        };

        this.sounds.warning = () => {
            const oscillator = this.audioContext.createOscillator();
            const gainNode = this.audioContext.createGain();

            oscillator.connect(gainNode);
            gainNode.connect(this.audioContext.destination);

            oscillator.frequency.value = 500;
            oscillator.type = 'sawtooth';

            gainNode.gain.setValueAtTime(0.15, this.audioContext.currentTime);
            gainNode.gain.exponentialRampToValueAtTime(0.01, this.audioContext.currentTime + 0.15);

            oscillator.start(this.audioContext.currentTime);
            oscillator.stop(this.audioContext.currentTime + 0.15);
        };
    }

    setupControls() {
        // Sound toggle
        const soundToggle = document.getElementById('notification-sound-toggle');
        if (soundToggle) {
            soundToggle.addEventListener('change', (e) => {
                this.soundEnabled = e.target.checked;
                this.saveSettings();
            });
        }

        // Clear all button
        const clearBtn = document.getElementById('notification-clear-all');
        if (clearBtn) {
            clearBtn.addEventListener('click', () => this.clearAll());
        }

        // Badge click to clear
        if (this.badge) {
            this.badge.addEventListener('click', () => {
                this.resetBadge();
            });
        }
    }

    restoreSettings() {
        try {
            const saved = localStorage.getItem('aingle-notification-settings');
            if (saved) {
                const settings = JSON.parse(saved);
                this.soundEnabled = settings.soundEnabled || false;

                const soundToggle = document.getElementById('notification-sound-toggle');
                if (soundToggle) {
                    soundToggle.checked = this.soundEnabled;
                }
            }
        } catch (error) {
            console.error('Failed to restore notification settings:', error);
        }
    }

    saveSettings() {
        try {
            const settings = {
                soundEnabled: this.soundEnabled
            };
            localStorage.setItem('aingle-notification-settings', JSON.stringify(settings));
        } catch (error) {
            console.error('Failed to save notification settings:', error);
        }
    }

    /**
     * Show a notification
     * @param {string} message - The notification message
     * @param {string} type - Type: 'info', 'success', 'warning', 'error'
     * @param {number} duration - Duration in milliseconds (0 = persistent)
     * @param {object} options - Additional options
     */
    show(message, type = 'info', duration = 5000, options = {}) {
        const id = `notification-${Date.now()}-${Math.random()}`;

        const notification = {
            id,
            message,
            type,
            timestamp: Date.now(),
            persistent: duration === 0,
            options
        };

        this.notifications.push(notification);

        // Create notification element
        const element = this.createNotificationElement(notification);
        this.container.appendChild(element);

        // Play sound
        if (this.soundEnabled && this.sounds[type]) {
            try {
                this.sounds[type]();
            } catch (error) {
                console.error('Failed to play sound:', error);
            }
        }

        // Auto-remove after duration
        if (duration > 0) {
            setTimeout(() => {
                this.remove(id);
            }, duration);
        }

        // Limit number of visible notifications
        this.limitNotifications();

        // Increment badge
        this.incrementBadge();

        return id;
    }

    createNotificationElement(notification) {
        const element = document.createElement('div');
        element.id = notification.id;
        element.className = `notification notification-${notification.type}`;

        // Icon
        const icon = this.getIcon(notification.type);

        // Close button
        const closeBtn = document.createElement('button');
        closeBtn.className = 'notification-close';
        closeBtn.innerHTML = '&times;';
        closeBtn.addEventListener('click', () => this.remove(notification.id));

        // Message
        const message = document.createElement('div');
        message.className = 'notification-message';
        message.textContent = notification.message;

        // Assemble
        element.innerHTML = `
            <div class="notification-icon">${icon}</div>
            <div class="notification-content">
                <div class="notification-message">${this.escapeHtml(notification.message)}</div>
                ${notification.options.subtitle ? `<div class="notification-subtitle">${this.escapeHtml(notification.options.subtitle)}</div>` : ''}
            </div>
        `;
        element.appendChild(closeBtn);

        // Animation
        setTimeout(() => {
            element.classList.add('notification-show');
        }, 10);

        return element;
    }

    getIcon(type) {
        const icons = {
            info: '<svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor"><circle cx="10" cy="10" r="8" fill="none" stroke="currentColor" stroke-width="2"/><path d="M10 14v-4M10 7v-1"/></svg>',
            success: '<svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor"><path d="M10 2a8 8 0 100 16 8 8 0 000-16zm-2 11l-3-3 1.41-1.41L8 10.17l4.59-4.58L14 7l-6 6z"/></svg>',
            warning: '<svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor"><path d="M10 2L2 18h16L10 2zm0 13a1 1 0 110-2 1 1 0 010 2zm0-3a1 1 0 01-1-1V9a1 1 0 012 0v2a1 1 0 01-1 1z"/></svg>',
            error: '<svg width="20" height="20" viewBox="0 0 20 20" fill="currentColor"><path d="M10 2a8 8 0 100 16 8 8 0 000-16zm3.707 11.293a1 1 0 01-1.414 1.414L10 12.414l-2.293 2.293a1 1 0 01-1.414-1.414L8.586 11 6.293 8.707a1 1 0 011.414-1.414L10 9.586l2.293-2.293a1 1 0 011.414 1.414L11.414 11l2.293 2.293z"/></svg>'
        };
        return icons[type] || icons.info;
    }

    remove(id) {
        const element = document.getElementById(id);
        if (element) {
            element.classList.remove('notification-show');
            element.classList.add('notification-hide');

            setTimeout(() => {
                element.remove();
            }, 300);
        }

        this.notifications = this.notifications.filter(n => n.id !== id);
    }

    clearAll() {
        this.notifications.forEach(n => this.remove(n.id));
        this.notifications = [];
        this.resetBadge();
    }

    limitNotifications() {
        while (this.container.children.length > this.maxNotifications) {
            const oldest = this.container.firstChild;
            if (oldest) {
                this.remove(oldest.id);
            }
        }
    }

    incrementBadge() {
        this.badgeCount++;
        this.updateBadge();
        this.updateDocumentTitle();
    }

    resetBadge() {
        this.badgeCount = 0;
        this.updateBadge();
        this.updateDocumentTitle();
    }

    updateBadge() {
        if (this.badge) {
            this.badge.textContent = this.badgeCount;
            if (this.badgeCount > 0) {
                this.badge.classList.remove('hidden');
            } else {
                this.badge.classList.add('hidden');
            }
        }
    }

    updateDocumentTitle() {
        const baseTitle = 'AIngle DAG Visualization';
        if (this.badgeCount > 0) {
            document.title = `(${this.badgeCount}) ${baseTitle}`;
        } else {
            document.title = baseTitle;
        }
    }

    // Preset notification types
    success(message, duration = 5000) {
        return this.show(message, 'success', duration);
    }

    error(message, duration = 7000) {
        return this.show(message, 'error', duration);
    }

    warning(message, duration = 6000) {
        return this.show(message, 'warning', duration);
    }

    info(message, duration = 5000) {
        return this.show(message, 'info', duration);
    }

    // DAG-specific notifications
    nodeAdded(node) {
        return this.show(
            `New ${node.node_type} node added`,
            'info',
            3000,
            { subtitle: `ID: ${node.id.substring(0, 20)}...` }
        );
    }

    edgeAdded(edge) {
        return this.show(
            'New edge added to DAG',
            'info',
            2000
        );
    }

    connected() {
        return this.show(
            'Connected to server',
            'success',
            3000
        );
    }

    disconnected() {
        return this.show(
            'Disconnected from server',
            'warning',
            0  // Persistent until reconnected
        );
    }

    reconnecting() {
        return this.show(
            'Reconnecting to server...',
            'info',
            3000
        );
    }

    // Utility
    escapeHtml(text) {
        const div = document.createElement('div');
        div.textContent = text;
        return div.innerHTML;
    }

    // Enable/disable sounds
    setSoundEnabled(enabled) {
        this.soundEnabled = enabled;
        this.saveSettings();

        const soundToggle = document.getElementById('notification-sound-toggle');
        if (soundToggle) {
            soundToggle.checked = enabled;
        }
    }

    getSoundEnabled() {
        return this.soundEnabled;
    }
}

window.NotificationManager = NotificationManager;
