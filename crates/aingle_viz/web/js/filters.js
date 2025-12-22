/**
 * Advanced Filtering System for DAG Visualization
 * Supports filtering by agent, entry type, date range, and content search
 */

class FilterManager {
    constructor() {
        this.filters = {
            agents: new Set(),
            types: new Set(['genesis', 'entry', 'action', 'link', 'agent', 'create', 'update', 'delete', 'system']),
            dateRange: null,
            searchTerm: '',
            customFilters: []
        };

        this.listeners = [];
        this.debounceTimer = null;
        this.init();
    }

    init() {
        this.setupTypeFilters();
        this.setupAgentFilters();
        this.setupDateRangeFilter();
        this.setupSearchFilter();
        this.setupQuickFilters();
    }

    setupTypeFilters() {
        const container = document.getElementById('filter-types');
        if (!container) return;

        const types = [
            { id: 'genesis', label: 'Genesis', color: '#ff6b9d' },
            { id: 'entry', label: 'Entry', color: '#4ecdc4' },
            { id: 'action', label: 'Action', color: '#45b7d1' },
            { id: 'create', label: 'Create', color: '#4ecdc4' },
            { id: 'update', label: 'Update', color: '#45b7d1' },
            { id: 'delete', label: 'Delete', color: '#f38181' },
            { id: 'link', label: 'Link', color: '#ffe66d' },
            { id: 'agent', label: 'Agent', color: '#a8e6cf' },
            { id: 'system', label: 'System', color: '#607D8B' }
        ];

        container.innerHTML = types.map(type => `
            <label class="filter-checkbox">
                <input type="checkbox"
                       id="filter-type-${type.id}"
                       value="${type.id}"
                       ${this.filters.types.has(type.id) ? 'checked' : ''}>
                <span class="filter-color" style="background-color: ${type.color}"></span>
                <span class="filter-label">${type.label}</span>
            </label>
        `).join('');

        types.forEach(type => {
            const checkbox = document.getElementById(`filter-type-${type.id}`);
            if (checkbox) {
                checkbox.addEventListener('change', () => {
                    if (checkbox.checked) {
                        this.filters.types.add(type.id);
                    } else {
                        this.filters.types.delete(type.id);
                    }
                    this.notifyChange();
                });
            }
        });
    }

    setupAgentFilters() {
        // Agent filters are dynamically updated when agents are discovered
        const container = document.getElementById('filter-agents');
        if (container) {
            container.innerHTML = '<p class="filter-placeholder">No agents detected yet...</p>';
        }
    }

    updateAgentFilters(agents) {
        const container = document.getElementById('filter-agents');
        if (!container) return;

        if (agents.length === 0) {
            container.innerHTML = '<p class="filter-placeholder">No agents detected yet...</p>';
            return;
        }

        container.innerHTML = `
            <div class="filter-agent-controls">
                <button class="btn btn-sm" id="agent-select-all">Select All</button>
                <button class="btn btn-sm" id="agent-deselect-all">Deselect All</button>
            </div>
            <div class="filter-agent-list" id="agent-checkbox-list"></div>
        `;

        const listContainer = document.getElementById('agent-checkbox-list');

        agents.forEach(agent => {
            const isChecked = this.filters.agents.size === 0 || this.filters.agents.has(agent.id);
            const checkbox = document.createElement('label');
            checkbox.className = 'filter-checkbox';
            checkbox.innerHTML = `
                <input type="checkbox"
                       id="filter-agent-${agent.id}"
                       value="${agent.id}"
                       ${isChecked ? 'checked' : ''}>
                <span class="filter-color" style="background-color: ${agent.color}"></span>
                <span class="filter-label" title="${agent.id}">${this.truncateId(agent.id)}</span>
                <span class="filter-count">(${agent.count})</span>
            `;
            listContainer.appendChild(checkbox);

            const input = checkbox.querySelector('input');
            input.addEventListener('change', () => {
                if (input.checked) {
                    this.filters.agents.delete(agent.id);
                } else {
                    this.filters.agents.add(agent.id);
                }
                // If all are unchecked, show all
                if (this.filters.agents.size === agents.length) {
                    this.filters.agents.clear();
                }
                this.notifyChange();
            });
        });

        // Select/Deselect all buttons
        document.getElementById('agent-select-all').addEventListener('click', () => {
            this.filters.agents.clear();
            listContainer.querySelectorAll('input[type="checkbox"]').forEach(cb => {
                cb.checked = true;
            });
            this.notifyChange();
        });

        document.getElementById('agent-deselect-all').addEventListener('click', () => {
            agents.forEach(a => this.filters.agents.add(a.id));
            listContainer.querySelectorAll('input[type="checkbox"]').forEach(cb => {
                cb.checked = false;
            });
            this.notifyChange();
        });
    }

    setupDateRangeFilter() {
        const startInput = document.getElementById('filter-date-start');
        const endInput = document.getElementById('filter-date-end');
        const resetBtn = document.getElementById('filter-date-reset');

        if (!startInput || !endInput) return;

        const applyDateFilter = () => {
            const start = startInput.value ? new Date(startInput.value).getTime() : null;
            const end = endInput.value ? new Date(endInput.value).getTime() : null;

            if (start || end) {
                this.filters.dateRange = { start, end };
            } else {
                this.filters.dateRange = null;
            }
            this.notifyChange();
        };

        startInput.addEventListener('change', applyDateFilter);
        endInput.addEventListener('change', applyDateFilter);

        if (resetBtn) {
            resetBtn.addEventListener('click', () => {
                startInput.value = '';
                endInput.value = '';
                this.filters.dateRange = null;
                this.notifyChange();
            });
        }
    }

    setupSearchFilter() {
        const searchInput = document.getElementById('filter-search');
        if (!searchInput) return;

        searchInput.addEventListener('input', () => {
            clearTimeout(this.debounceTimer);
            this.debounceTimer = setTimeout(() => {
                this.filters.searchTerm = searchInput.value.toLowerCase().trim();
                this.notifyChange();
            }, 300);
        });

        // Clear button
        const clearBtn = document.getElementById('filter-search-clear');
        if (clearBtn) {
            clearBtn.addEventListener('click', () => {
                searchInput.value = '';
                this.filters.searchTerm = '';
                this.notifyChange();
            });
        }
    }

    setupQuickFilters() {
        const container = document.getElementById('quick-filters');
        if (!container) return;

        const quickFilters = [
            {
                label: 'Recent (24h)',
                action: () => {
                    const now = Date.now();
                    const dayAgo = now - 24 * 60 * 60 * 1000;
                    this.filters.dateRange = { start: dayAgo, end: now };
                    this.notifyChange();
                }
            },
            {
                label: 'Genesis Only',
                action: () => {
                    this.filters.types.clear();
                    this.filters.types.add('genesis');
                    this.updateTypeCheckboxes();
                    this.notifyChange();
                }
            },
            {
                label: 'Entries Only',
                action: () => {
                    this.filters.types.clear();
                    this.filters.types.add('entry');
                    this.filters.types.add('create');
                    this.filters.types.add('update');
                    this.filters.types.add('delete');
                    this.updateTypeCheckboxes();
                    this.notifyChange();
                }
            },
            {
                label: 'Reset All',
                action: () => this.reset()
            }
        ];

        container.innerHTML = quickFilters.map((filter, index) => `
            <button class="btn btn-sm quick-filter-btn" id="quick-filter-${index}">
                ${filter.label}
            </button>
        `).join('');

        quickFilters.forEach((filter, index) => {
            document.getElementById(`quick-filter-${index}`).addEventListener('click', filter.action);
        });
    }

    updateTypeCheckboxes() {
        document.querySelectorAll('[id^="filter-type-"]').forEach(checkbox => {
            const type = checkbox.value;
            checkbox.checked = this.filters.types.has(type);
        });
    }

    // Filter methods
    byAgent(agentId) {
        if (!agentId) {
            this.filters.agents.clear();
        } else if (Array.isArray(agentId)) {
            this.filters.agents = new Set(agentId);
        } else {
            this.filters.agents = new Set([agentId]);
        }
        this.notifyChange();
        return this;
    }

    byEntryType(types) {
        if (!types) {
            this.filters.types = new Set(['genesis', 'entry', 'action', 'link', 'agent', 'create', 'update', 'delete']);
        } else if (Array.isArray(types)) {
            this.filters.types = new Set(types);
        } else {
            this.filters.types = new Set([types]);
        }
        this.updateTypeCheckboxes();
        this.notifyChange();
        return this;
    }

    byDateRange(start, end) {
        if (!start && !end) {
            this.filters.dateRange = null;
        } else {
            this.filters.dateRange = { start, end };
        }
        this.notifyChange();
        return this;
    }

    byContent(searchTerm) {
        this.filters.searchTerm = searchTerm ? searchTerm.toLowerCase().trim() : '';
        const searchInput = document.getElementById('filter-search');
        if (searchInput) {
            searchInput.value = searchTerm || '';
        }
        this.notifyChange();
        return this;
    }

    addCustomFilter(name, filterFn) {
        this.filters.customFilters.push({ name, filterFn });
        this.notifyChange();
        return this;
    }

    removeCustomFilter(name) {
        this.filters.customFilters = this.filters.customFilters.filter(f => f.name !== name);
        this.notifyChange();
        return this;
    }

    // Apply all filters to a dataset
    apply(nodes) {
        let filtered = nodes;

        // Filter by agents (if any are specifically selected)
        if (this.filters.agents.size > 0) {
            filtered = filtered.filter(n => !this.filters.agents.has(n.agent_id));
        }

        // Filter by type
        if (this.filters.types.size > 0) {
            filtered = filtered.filter(n => this.filters.types.has(n.node_type));
        }

        // Filter by date range
        if (this.filters.dateRange) {
            const { start, end } = this.filters.dateRange;
            filtered = filtered.filter(n => {
                if (!n.timestamp) return false;
                const nodeTime = n.timestamp * 1000;
                if (start && nodeTime < start) return false;
                if (end && nodeTime > end) return false;
                return true;
            });
        }

        // Filter by search term
        if (this.filters.searchTerm) {
            filtered = filtered.filter(n => {
                const searchIn = [
                    n.id,
                    n.label,
                    n.node_type,
                    n.author,
                    JSON.stringify(n.content || {})
                ].join(' ').toLowerCase();
                return searchIn.includes(this.filters.searchTerm);
            });
        }

        // Apply custom filters
        this.filters.customFilters.forEach(({ filterFn }) => {
            filtered = filtered.filter(filterFn);
        });

        return filtered;
    }

    // Get current filter state
    getState() {
        return {
            agents: Array.from(this.filters.agents),
            types: Array.from(this.filters.types),
            dateRange: this.filters.dateRange,
            searchTerm: this.filters.searchTerm,
            customFilters: this.filters.customFilters.map(f => f.name)
        };
    }

    // Restore filter state
    setState(state) {
        if (state.agents) this.filters.agents = new Set(state.agents);
        if (state.types) this.filters.types = new Set(state.types);
        if (state.dateRange) this.filters.dateRange = state.dateRange;
        if (state.searchTerm !== undefined) this.filters.searchTerm = state.searchTerm;
        this.updateTypeCheckboxes();
        this.notifyChange();
    }

    // Reset all filters
    reset() {
        this.filters.agents.clear();
        this.filters.types = new Set(['genesis', 'entry', 'action', 'link', 'agent', 'create', 'update', 'delete', 'system']);
        this.filters.dateRange = null;
        this.filters.searchTerm = '';
        this.filters.customFilters = [];

        // Reset UI
        this.updateTypeCheckboxes();
        const searchInput = document.getElementById('filter-search');
        if (searchInput) searchInput.value = '';

        const dateStart = document.getElementById('filter-date-start');
        const dateEnd = document.getElementById('filter-date-end');
        if (dateStart) dateStart.value = '';
        if (dateEnd) dateEnd.value = '';

        this.notifyChange();
    }

    // Event listeners
    onChange(callback) {
        this.listeners.push(callback);
        return () => {
            this.listeners = this.listeners.filter(cb => cb !== callback);
        };
    }

    notifyChange() {
        const state = this.getState();
        this.listeners.forEach(callback => callback(state, this));
    }

    // Helper methods
    truncateId(id, maxLength = 20) {
        if (id.length <= maxLength) return id;
        return id.substring(0, maxLength - 3) + '...';
    }

    // Export filter stats
    getStats() {
        return {
            activeFilters: {
                agents: this.filters.agents.size,
                types: this.filters.types.size,
                dateRange: this.filters.dateRange ? 'active' : 'inactive',
                search: this.filters.searchTerm ? 'active' : 'inactive',
                custom: this.filters.customFilters.length
            },
            totalFilters: this.filters.agents.size +
                         (this.filters.dateRange ? 1 : 0) +
                         (this.filters.searchTerm ? 1 : 0) +
                         this.filters.customFilters.length
        };
    }
}

window.FilterManager = FilterManager;
