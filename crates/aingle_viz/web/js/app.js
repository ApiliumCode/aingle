/**
 * AIngle DAG Visualization - Main Application
 */

class App {
    constructor() {
        this.graph = null;
        this.ws = null;
        this.filters = {
            genesis: true,
            create: true,
            update: true,
            delete: true,
            link: true,
            agent: true
        };

        this.init();
    }

    init() {
        // Initialize graph
        this.graph = new DagGraph('dag-graph');

        // Initialize WebSocket
        const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${wsProtocol}//${window.location.host}/ws/updates`;
        this.ws = new WebSocketManager(wsUrl);

        // Setup WebSocket handlers
        this.ws.on('open', () => this.onConnected());
        this.ws.on('close', () => this.onDisconnected());
        this.ws.on('initial', (data) => this.onInitialData(data));
        this.ws.on('node_added', (data) => this.onNodeAdded(data));

        // Connect
        this.ws.connect();

        // Setup UI handlers
        this.setupControls();
        this.setupFilters();
        this.setupNodeDetails();

        // Load initial data via REST API as fallback
        this.loadInitialData();
    }

    async loadInitialData() {
        try {
            const response = await fetch('/api/dag');
            const data = await response.json();
            this.graph.setData(data);
            this.updateStats();
            this.updateAgentList();
        } catch (error) {
            console.error('Failed to load initial data:', error);
        }
    }

    onConnected() {
        const status = document.getElementById('connection-status');
        status.textContent = 'Connected';
        status.classList.add('connected');
    }

    onDisconnected() {
        const status = document.getElementById('connection-status');
        status.textContent = 'Disconnected';
        status.classList.remove('connected');
    }

    onInitialData(data) {
        this.graph.setData(data);
        this.updateStats();
        this.updateAgentList();
    }

    onNodeAdded(data) {
        if (data.node) {
            this.graph.addNode(data.node, data.edges || []);
            this.updateStats();
            this.updateAgentList();
        }
    }

    setupControls() {
        // Reset view button
        document.getElementById('btn-reset').addEventListener('click', () => {
            this.graph.resetView();
        });

        // Pause button
        const pauseBtn = document.getElementById('btn-pause');
        pauseBtn.addEventListener('click', () => {
            const isPaused = this.graph.togglePause();
            pauseBtn.textContent = isPaused ? 'Resume' : 'Pause';
            pauseBtn.classList.toggle('active', isPaused);
        });

        // Export button
        document.getElementById('btn-export').addEventListener('click', () => {
            this.graph.exportSVG();
        });
    }

    setupFilters() {
        const filterTypes = ['create', 'update', 'delete', 'link', 'agent'];

        filterTypes.forEach(type => {
            const checkbox = document.getElementById(`filter-${type}`);
            if (checkbox) {
                checkbox.addEventListener('change', () => {
                    this.filters[type] = checkbox.checked;
                    this.applyFilters();
                });
            }
        });
    }

    applyFilters() {
        const enabledTypes = Object.entries(this.filters)
            .filter(([_, enabled]) => enabled)
            .map(([type, _]) => type);
        this.graph.filterByType(enabledTypes);
    }

    setupNodeDetails() {
        window.addEventListener('nodeSelected', (event) => {
            this.showNodeDetails(event.detail);
        });

        // Click on graph background to deselect
        document.getElementById('dag-graph').addEventListener('click', (event) => {
            if (event.target.tagName === 'svg') {
                this.clearNodeDetails();
            }
        });
    }

    showNodeDetails(node) {
        const container = document.getElementById('node-details');
        container.innerHTML = `
            <div class="detail-row">
                <span class="detail-label">ID</span>
                <span class="detail-value" title="${node.id}">${node.id}</span>
            </div>
            <div class="detail-row">
                <span class="detail-label">Hash</span>
                <span class="detail-value" title="${node.hash}">${node.hash}</span>
            </div>
            <div class="detail-row">
                <span class="detail-label">Type</span>
                <span class="detail-value">${node.node_type}</span>
            </div>
            <div class="detail-row">
                <span class="detail-label">Agent</span>
                <span class="detail-value">${node.agent_id}</span>
            </div>
            <div class="detail-row">
                <span class="detail-label">Parents</span>
                <span class="detail-value">${node.parents ? node.parents.length : 0}</span>
            </div>
            <div class="content-preview">${JSON.stringify(node.content, null, 2)}</div>
        `;
    }

    clearNodeDetails() {
        const container = document.getElementById('node-details');
        container.innerHTML = '<p class="placeholder">Click a node to view details</p>';
    }

    updateStats() {
        const nodes = this.graph.nodes;
        const edges = this.graph.edges;
        const agents = new Set(nodes.map(n => n.agent_id));

        document.getElementById('stat-nodes').textContent = nodes.length;
        document.getElementById('stat-edges').textContent = edges.length;
        document.getElementById('stat-agents').textContent = agents.size;
    }

    updateAgentList() {
        const container = document.getElementById('agent-list');
        const agents = this.graph.getAgents();

        container.innerHTML = agents.map(agent => `
            <div class="agent-item" data-agent="${agent.id}">
                <span class="agent-color" style="background: ${agent.color}"></span>
                <span class="agent-name" title="${agent.id}">${agent.id}</span>
                <span class="agent-count">${agent.count}</span>
            </div>
        `).join('');

        // Add click handlers
        container.querySelectorAll('.agent-item').forEach(item => {
            item.addEventListener('click', () => {
                const agentId = item.dataset.agent;

                // Toggle selection
                const wasSelected = item.classList.contains('selected');
                container.querySelectorAll('.agent-item').forEach(i => i.classList.remove('selected'));

                if (!wasSelected) {
                    item.classList.add('selected');
                    this.graph.filterByAgent(agentId);
                } else {
                    this.graph.filterByAgent(null);
                }
            });
        });
    }
}

// Start application
document.addEventListener('DOMContentLoaded', () => {
    window.app = new App();
});
