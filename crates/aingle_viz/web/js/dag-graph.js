/**
 * DAG Graph Visualization using D3.js
 * Organic neural network style physics
 */

class DagGraph {
    constructor(containerId) {
        this.container = d3.select(`#${containerId}`);
        this.nodes = [];
        this.edges = [];
        this.simulation = null;
        this.svg = null;
        this.g = null;
        this.zoom = null;
        this.selectedNode = null;
        this.paused = false;

        // Neural network inspired color palette
        this.typeColors = {
            genesis: '#ff6b9d',  // Pink core
            create: '#4ecdc4',   // Teal neurons
            update: '#45b7d1',   // Cyan synapses
            delete: '#f38181',   // Soft red
            link: '#ffe66d',     // Yellow connections
            agent: '#a8e6cf'     // Mint green
        };

        // Agent colors - use a softer, more organic palette
        this.agentColors = {};
        this.colorScale = d3.scaleOrdinal([
            '#ff6b6b', '#4ecdc4', '#45b7d1', '#96ceb4', '#ffeaa7',
            '#dfe6e9', '#fd79a8', '#a29bfe', '#6c5ce7', '#00b894',
            '#e17055', '#74b9ff', '#55efc4', '#81ecec', '#fab1a0',
            '#ff7675', '#fdcb6e', '#e84393', '#00cec9', '#0984e3'
        ]);

        this.init();
    }

    init() {
        const rect = this.container.node().getBoundingClientRect();
        const width = rect.width;
        const height = rect.height;

        // Setup SVG with gradient definitions
        this.svg = this.container
            .attr('width', width)
            .attr('height', height);

        // Add glow filter for neural effect
        const defs = this.svg.append('defs');

        // Glow filter
        const filter = defs.append('filter')
            .attr('id', 'glow')
            .attr('x', '-50%')
            .attr('y', '-50%')
            .attr('width', '200%')
            .attr('height', '200%');

        filter.append('feGaussianBlur')
            .attr('stdDeviation', '2')
            .attr('result', 'coloredBlur');

        const feMerge = filter.append('feMerge');
        feMerge.append('feMergeNode').attr('in', 'coloredBlur');
        feMerge.append('feMergeNode').attr('in', 'SourceGraphic');

        // Softer glow for links
        const linkFilter = defs.append('filter')
            .attr('id', 'link-glow')
            .attr('x', '-50%')
            .attr('y', '-50%')
            .attr('width', '200%')
            .attr('height', '200%');

        linkFilter.append('feGaussianBlur')
            .attr('stdDeviation', '1')
            .attr('result', 'coloredBlur');

        // Create main group for zoom/pan
        this.g = this.svg.append('g');

        // Create layers
        this.edgesGroup = this.g.append('g').attr('class', 'edges');
        this.nodesGroup = this.g.append('g').attr('class', 'nodes');

        // Setup zoom behavior
        this.zoom = d3.zoom()
            .scaleExtent([0.05, 8])
            .on('zoom', (event) => {
                this.g.attr('transform', event.transform);
            });

        this.svg.call(this.zoom);

        // ORGANIC NEURAL NETWORK PHYSICS
        this.simulation = d3.forceSimulation()
            // Links: variable distance based on connection type
            .force('link', d3.forceLink()
                .id(d => d.id)
                .distance(d => {
                    // Shorter links for same-agent connections (source chain)
                    if (d.source.agent_id === d.target.agent_id) return 25;
                    // Longer links for cross-agent connections
                    return 60;
                })
                .strength(d => {
                    // Stronger bonds within same agent
                    if (d.source.agent_id === d.target.agent_id) return 0.8;
                    return 0.3;
                }))
            // Charge: weak repulsion for organic clustering
            .force('charge', d3.forceManyBody()
                .strength(d => {
                    if (d.node_type === 'genesis') return -100;
                    if (d.node_type === 'agent') return -40;
                    return -15;  // Weak repulsion allows clustering
                })
                .distanceMin(5)
                .distanceMax(300))
            // Weak center force - let the network float
            .force('center', d3.forceCenter(width / 2, height / 2).strength(0.02))
            // Small collision radius for tight packing
            .force('collision', d3.forceCollide()
                .radius(d => this.getNodeRadius(d) + 2)
                .strength(0.7))
            // Add radial force to create organic clusters
            .force('radial', d3.forceRadial(
                d => {
                    if (d.node_type === 'genesis') return 0;
                    if (d.node_type === 'agent') return 100;
                    return 200 + Math.random() * 100;
                },
                width / 2,
                height / 2
            ).strength(0.01))
            // Velocity decay for smooth, organic movement
            .velocityDecay(0.4)
            .alphaDecay(0.01)  // Slower decay for continuous subtle movement
            .on('tick', () => this.tick());

        // Handle window resize
        window.addEventListener('resize', () => this.handleResize());

        // Initial zoom out to see the whole network
        this.svg.call(this.zoom.transform, d3.zoomIdentity.translate(width/2, height/2).scale(0.5).translate(-width/2, -height/2));
    }

    handleResize() {
        const rect = this.container.node().getBoundingClientRect();
        this.svg
            .attr('width', rect.width)
            .attr('height', rect.height);

        this.simulation
            .force('center', d3.forceCenter(rect.width / 2, rect.height / 2).strength(0.02))
            .alpha(0.1)
            .restart();
    }

    getAgentColor(agentId) {
        if (!this.agentColors[agentId]) {
            // Use hash-based color for consistency
            const hash = agentId.split('').reduce((a, b) => ((a << 5) - a) + b.charCodeAt(0), 0);
            this.agentColors[agentId] = this.colorScale(Math.abs(hash) % 20);
        }
        return this.agentColors[agentId];
    }

    getNodeColor(node) {
        return this.getAgentColor(node.agent_id);
    }

    getNodeRadius(node) {
        switch (node.node_type) {
            case 'genesis': return 12;
            case 'agent': return 5;
            case 'link': return 4;
            default: return 3;  // Smaller nodes for neural look
        }
    }

    setData(data) {
        this.nodes = data.nodes || [];
        this.edges = data.edges || [];
        this.update();
    }

    addNode(node, newEdges = []) {
        if (this.nodes.find(n => n.id === node.id)) return;

        // Position near parents with organic spread
        if (node.parents && node.parents.length > 0) {
            const parent = this.nodes.find(n => n.id === node.parents[0]);
            if (parent) {
                const angle = Math.random() * Math.PI * 2;
                const distance = 20 + Math.random() * 30;
                node.x = parent.x + Math.cos(angle) * distance;
                node.y = parent.y + Math.sin(angle) * distance;
            }
        }

        this.nodes.push(node);
        this.edges.push(...newEdges);
        this.update(true);
    }

    update(animate = false) {
        // Update edges with organic styling
        const edgeSelection = this.edgesGroup
            .selectAll('line')
            .data(this.edges, d => `${d.source.id || d.source}-${d.target.id || d.target}`);

        edgeSelection.exit().remove();

        edgeSelection.enter()
            .append('line')
            .attr('class', 'link')
            .attr('stroke', d => {
                // Color links by source agent
                const sourceNode = typeof d.source === 'object' ? d.source : this.nodes.find(n => n.id === d.source);
                return sourceNode ? this.getAgentColor(sourceNode.agent_id) : '#666';
            })
            .attr('stroke-opacity', 0.3)
            .attr('stroke-width', 0.5)
            .style('filter', 'url(#link-glow)');

        // Update nodes with neural styling
        const nodeSelection = this.nodesGroup
            .selectAll('g.node')
            .data(this.nodes, d => d.id);

        nodeSelection.exit().remove();

        const nodeEnter = nodeSelection.enter()
            .append('g')
            .attr('class', d => `node ${animate ? 'new' : ''}`)
            .call(d3.drag()
                .on('start', (event, d) => this.dragStarted(event, d))
                .on('drag', (event, d) => this.dragged(event, d))
                .on('end', (event, d) => this.dragEnded(event, d)))
            .on('click', (event, d) => this.nodeClicked(event, d))
            .on('mouseover', (event, d) => this.showTooltip(event, d))
            .on('mouseout', () => this.hideTooltip());

        nodeEnter.append('circle')
            .attr('r', d => this.getNodeRadius(d))
            .attr('fill', d => this.getNodeColor(d))
            .attr('stroke', d => d3.color(this.getNodeColor(d)).brighter(0.5))
            .attr('stroke-width', 0.5)
            .style('filter', 'url(#glow)');

        // Only show labels for genesis and on hover
        nodeEnter.append('text')
            .attr('dx', 8)
            .attr('dy', 3)
            .attr('font-size', '6px')
            .attr('fill', '#888')
            .attr('opacity', d => d.node_type === 'genesis' ? 1 : 0)
            .text(d => d.node_type === 'genesis' ? 'GENESIS' : '');

        // Update simulation
        this.simulation
            .nodes(this.nodes)
            .alpha(animate ? 0.3 : 0.1)
            .restart();

        this.simulation.force('link').links(this.edges);
    }

    tick() {
        if (this.paused) return;

        this.edgesGroup.selectAll('line')
            .attr('x1', d => d.source.x)
            .attr('y1', d => d.source.y)
            .attr('x2', d => d.target.x)
            .attr('y2', d => d.target.y);

        this.nodesGroup.selectAll('g.node')
            .attr('transform', d => `translate(${d.x},${d.y})`);
    }

    dragStarted(event, d) {
        if (!event.active) this.simulation.alphaTarget(0.2).restart();
        d.fx = d.x;
        d.fy = d.y;
    }

    dragged(event, d) {
        d.fx = event.x;
        d.fy = event.y;
    }

    dragEnded(event, d) {
        if (!event.active) this.simulation.alphaTarget(0);
        d.fx = null;
        d.fy = null;
    }

    nodeClicked(event, d) {
        event.stopPropagation();

        this.nodesGroup.selectAll('g.node').classed('selected', false);
        this.edgesGroup.selectAll('line').classed('highlighted', false);

        this.selectedNode = d;
        d3.select(event.currentTarget).classed('selected', true);

        // Highlight connected edges with pulse effect
        this.edgesGroup.selectAll('line')
            .attr('stroke-opacity', e =>
                (e.source.id === d.id || e.target.id === d.id) ? 0.8 : 0.1)
            .attr('stroke-width', e =>
                (e.source.id === d.id || e.target.id === d.id) ? 1.5 : 0.5);

        window.dispatchEvent(new CustomEvent('nodeSelected', { detail: d }));
    }

    showTooltip(event, d) {
        // Highlight node on hover
        d3.select(event.currentTarget).select('circle')
            .attr('r', this.getNodeRadius(d) * 1.5);

        const tooltip = document.getElementById('tooltip');
        tooltip.innerHTML = `
            <div class="tooltip-title">${d.id}</div>
            <div class="tooltip-type">${d.node_type} | ${d.agent_id}</div>
        `;
        tooltip.classList.add('visible');
        tooltip.style.left = `${event.pageX + 15}px`;
        tooltip.style.top = `${event.pageY + 15}px`;
    }

    hideTooltip() {
        // Reset node size
        this.nodesGroup.selectAll('circle')
            .attr('r', d => this.getNodeRadius(d));

        const tooltip = document.getElementById('tooltip');
        tooltip.classList.remove('visible');

        // Reset edge highlighting if no node selected
        if (!this.selectedNode) {
            this.edgesGroup.selectAll('line')
                .attr('stroke-opacity', 0.3)
                .attr('stroke-width', 0.5);
        }
    }

    resetView() {
        const rect = this.container.node().getBoundingClientRect();
        this.svg.transition()
            .duration(750)
            .call(this.zoom.transform, d3.zoomIdentity
                .translate(rect.width / 2, rect.height / 2)
                .scale(0.3)
                .translate(-rect.width / 2, -rect.height / 2));
    }

    togglePause() {
        this.paused = !this.paused;
        if (!this.paused) {
            this.simulation.alpha(0.1).restart();
        }
        return this.paused;
    }

    exportSVG() {
        const svgElement = this.svg.node();
        const serializer = new XMLSerializer();
        let source = serializer.serializeToString(svgElement);
        source = '<?xml version="1.0" standalone="no"?>\r\n' + source;

        const blob = new Blob([source], { type: 'image/svg+xml;charset=utf-8' });
        const url = URL.createObjectURL(blob);

        const link = document.createElement('a');
        link.href = url;
        link.download = 'aingle-neural-dag.svg';
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);
    }

    getAgents() {
        const agents = {};
        this.nodes.forEach(node => {
            if (!agents[node.agent_id]) {
                agents[node.agent_id] = {
                    id: node.agent_id,
                    color: this.getAgentColor(node.agent_id),
                    count: 0
                };
            }
            agents[node.agent_id].count++;
        });
        return Object.values(agents).sort((a, b) => b.count - a.count);
    }

    filterByAgent(agentId) {
        this.nodesGroup.selectAll('g.node')
            .style('opacity', d => agentId ? (d.agent_id === agentId ? 1 : 0.05) : 1);
        this.edgesGroup.selectAll('line')
            .style('opacity', d => {
                if (!agentId) return 1;
                const src = typeof d.source === 'object' ? d.source : this.nodes.find(n => n.id === d.source);
                const tgt = typeof d.target === 'object' ? d.target : this.nodes.find(n => n.id === d.target);
                return (src?.agent_id === agentId || tgt?.agent_id === agentId) ? 1 : 0.02;
            });
    }

    filterByType(types) {
        this.nodesGroup.selectAll('g.node')
            .style('display', d => types.includes(d.node_type) ? 'block' : 'none');
    }
}

window.DagGraph = DagGraph;
