/**
 * Timeline View for DAG Visualization
 * Chronological visualization with temporal zoom controls
 */

class TimelineView {
    constructor(containerId) {
        this.container = d3.select(`#${containerId}`);
        this.nodes = [];
        this.svg = null;
        this.g = null;
        this.xAxis = null;
        this.yAxis = null;
        this.zoom = null;
        this.timeScale = d3.scaleTime();
        this.yScale = d3.scaleLinear();
        this.currentZoomLevel = 'day'; // day, week, month, year
        this.selectedAgents = new Set();
        this.selectedTypes = new Set(['genesis', 'entry', 'action', 'link', 'agent']);

        // Color scheme matching dag-graph.js
        this.typeColors = {
            genesis: '#ff6b9d',
            entry: '#4ecdc4',
            action: '#45b7d1',
            delete: '#f38181',
            link: '#ffe66d',
            agent: '#a8e6cf',
            create: '#4ecdc4',
            update: '#45b7d1',
            system: '#607D8B'
        };

        this.init();
    }

    init() {
        const rect = this.container.node().getBoundingClientRect();
        const margin = { top: 40, right: 30, bottom: 60, left: 80 };
        const width = rect.width - margin.left - margin.right;
        const height = rect.height - margin.top - margin.bottom;

        // Create SVG
        this.svg = this.container
            .attr('width', rect.width)
            .attr('height', rect.height);

        // Add gradient for background
        const defs = this.svg.append('defs');
        const gradient = defs.append('linearGradient')
            .attr('id', 'timeline-gradient')
            .attr('x1', '0%')
            .attr('x2', '0%')
            .attr('y1', '0%')
            .attr('y2', '100%');

        gradient.append('stop')
            .attr('offset', '0%')
            .attr('stop-color', '#1a1a2e')
            .attr('stop-opacity', 1);

        gradient.append('stop')
            .attr('offset', '100%')
            .attr('stop-color', '#16213e')
            .attr('stop-opacity', 1);

        this.svg.append('rect')
            .attr('width', rect.width)
            .attr('height', rect.height)
            .attr('fill', 'url(#timeline-gradient)');

        // Create main group
        this.g = this.svg.append('g')
            .attr('transform', `translate(${margin.left},${margin.top})`);

        // Create scales
        this.timeScale.range([0, width]);
        this.yScale.range([height, 0]);

        // Create axes
        this.xAxis = this.g.append('g')
            .attr('class', 'x-axis')
            .attr('transform', `translate(0,${height})`);

        this.yAxis = this.g.append('g')
            .attr('class', 'y-axis');

        // Add axis labels
        this.svg.append('text')
            .attr('class', 'axis-label')
            .attr('x', margin.left + width / 2)
            .attr('y', rect.height - 10)
            .attr('text-anchor', 'middle')
            .attr('fill', '#888')
            .text('Time');

        this.svg.append('text')
            .attr('class', 'axis-label')
            .attr('transform', 'rotate(-90)')
            .attr('x', -margin.top - height / 2)
            .attr('y', 15)
            .attr('text-anchor', 'middle')
            .attr('fill', '#888')
            .text('Entry Count');

        // Add zoom controls
        this.addZoomControls();

        // Handle window resize
        window.addEventListener('resize', () => this.handleResize());
    }

    addZoomControls() {
        const controls = d3.select('#timeline-controls');

        controls.html('');

        const zoomLevels = [
            { label: 'Day', value: 'day' },
            { label: 'Week', value: 'week' },
            { label: 'Month', value: 'month' },
            { label: 'Year', value: 'year' }
        ];

        zoomLevels.forEach(level => {
            controls.append('button')
                .attr('class', 'btn timeline-zoom-btn')
                .classed('active', level.value === this.currentZoomLevel)
                .text(level.label)
                .on('click', () => this.setZoomLevel(level.value));
        });
    }

    setZoomLevel(level) {
        this.currentZoomLevel = level;

        // Update button states
        d3.selectAll('.timeline-zoom-btn').classed('active', false);
        d3.select(`button:nth-child(${['day', 'week', 'month', 'year'].indexOf(level) + 1})`)
            .classed('active', true);

        this.update();
    }

    setData(nodes) {
        this.nodes = nodes.filter(n => n.timestamp);
        this.update();
    }

    filterByAgent(agentIds) {
        if (!agentIds || agentIds.length === 0) {
            this.selectedAgents.clear();
        } else {
            this.selectedAgents = new Set(agentIds);
        }
        this.update();
    }

    filterByType(types) {
        if (!types || types.length === 0) {
            this.selectedTypes = new Set(['genesis', 'entry', 'action', 'link', 'agent', 'create', 'update', 'delete']);
        } else {
            this.selectedTypes = new Set(types);
        }
        this.update();
    }

    filterByDateRange(startDate, endDate) {
        this.dateRange = { start: startDate, end: endDate };
        this.update();
    }

    getFilteredNodes() {
        let filtered = this.nodes;

        // Filter by agent
        if (this.selectedAgents.size > 0) {
            filtered = filtered.filter(n => this.selectedAgents.has(n.agent_id));
        }

        // Filter by type
        filtered = filtered.filter(n => this.selectedTypes.has(n.node_type));

        // Filter by date range
        if (this.dateRange) {
            filtered = filtered.filter(n => {
                const timestamp = n.timestamp * 1000;
                return timestamp >= this.dateRange.start && timestamp <= this.dateRange.end;
            });
        }

        return filtered;
    }

    aggregateByTime(nodes) {
        if (nodes.length === 0) return [];

        const aggregated = new Map();
        const timeFormat = this.getTimeFormat();

        nodes.forEach(node => {
            const date = new Date(node.timestamp * 1000);
            const key = this.getTimeKey(date);

            if (!aggregated.has(key)) {
                aggregated.set(key, {
                    date: this.getTimeBucket(date),
                    count: 0,
                    nodes: [],
                    types: {}
                });
            }

            const bucket = aggregated.get(key);
            bucket.count++;
            bucket.nodes.push(node);
            bucket.types[node.node_type] = (bucket.types[node.node_type] || 0) + 1;
        });

        return Array.from(aggregated.values()).sort((a, b) => a.date - b.date);
    }

    getTimeKey(date) {
        switch (this.currentZoomLevel) {
            case 'day':
                return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}-${date.getHours()}`;
            case 'week':
                return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
            case 'month':
                const weekOfMonth = Math.floor(date.getDate() / 7);
                return `${date.getFullYear()}-${date.getMonth()}-${weekOfMonth}`;
            case 'year':
                return `${date.getFullYear()}-${date.getMonth()}`;
            default:
                return date.toISOString();
        }
    }

    getTimeBucket(date) {
        const newDate = new Date(date);
        switch (this.currentZoomLevel) {
            case 'day':
                newDate.setMinutes(0, 0, 0);
                return newDate;
            case 'week':
                newDate.setHours(0, 0, 0, 0);
                return newDate;
            case 'month':
                newDate.setDate(Math.floor(date.getDate() / 7) * 7);
                newDate.setHours(0, 0, 0, 0);
                return newDate;
            case 'year':
                newDate.setDate(1);
                newDate.setHours(0, 0, 0, 0);
                return newDate;
            default:
                return newDate;
        }
    }

    getTimeFormat() {
        switch (this.currentZoomLevel) {
            case 'day':
                return d3.timeFormat('%H:%M');
            case 'week':
                return d3.timeFormat('%b %d');
            case 'month':
                return d3.timeFormat('%b %d');
            case 'year':
                return d3.timeFormat('%b %Y');
            default:
                return d3.timeFormat('%Y-%m-%d');
        }
    }

    update() {
        const filtered = this.getFilteredNodes();
        const aggregated = this.aggregateByTime(filtered);

        if (aggregated.length === 0) {
            this.g.selectAll('.timeline-bar').remove();
            this.g.selectAll('.timeline-point').remove();
            return;
        }

        // Update scales
        const dates = aggregated.map(d => d.date);
        const counts = aggregated.map(d => d.count);

        this.timeScale.domain(d3.extent(dates));
        this.yScale.domain([0, d3.max(counts) * 1.1]);

        // Update axes
        const xAxisFormat = this.getTimeFormat();
        this.xAxis.call(d3.axisBottom(this.timeScale).tickFormat(xAxisFormat))
            .selectAll('text')
            .attr('fill', '#888')
            .attr('transform', 'rotate(-45)')
            .style('text-anchor', 'end');

        this.yAxis.call(d3.axisLeft(this.yScale).ticks(5))
            .selectAll('text')
            .attr('fill', '#888');

        // Style axes
        this.xAxis.selectAll('line, path').attr('stroke', '#444');
        this.yAxis.selectAll('line, path').attr('stroke', '#444');

        // Calculate bar width
        const barWidth = Math.max(2, (this.timeScale.range()[1] / aggregated.length) - 2);

        // Draw bars
        const bars = this.g.selectAll('.timeline-bar')
            .data(aggregated, d => d.date.getTime());

        bars.exit()
            .transition()
            .duration(300)
            .attr('height', 0)
            .attr('y', this.yScale(0))
            .remove();

        const barsEnter = bars.enter()
            .append('rect')
            .attr('class', 'timeline-bar')
            .attr('x', d => this.timeScale(d.date) - barWidth / 2)
            .attr('width', barWidth)
            .attr('y', this.yScale(0))
            .attr('height', 0)
            .attr('fill', '#4fc3f7')
            .attr('opacity', 0.7)
            .attr('rx', 2)
            .on('mouseover', (event, d) => this.showBarTooltip(event, d))
            .on('mouseout', () => this.hideTooltip())
            .on('click', (event, d) => this.showBarDetails(event, d));

        bars.merge(barsEnter)
            .transition()
            .duration(300)
            .attr('x', d => this.timeScale(d.date) - barWidth / 2)
            .attr('width', barWidth)
            .attr('y', d => this.yScale(d.count))
            .attr('height', d => this.yScale(0) - this.yScale(d.count));
    }

    showBarTooltip(event, data) {
        const tooltip = d3.select('#timeline-tooltip');

        const typeBreakdown = Object.entries(data.types)
            .map(([type, count]) => `<div>${type}: ${count}</div>`)
            .join('');

        tooltip.html(`
            <div class="tooltip-title">${this.getTimeFormat()(data.date)}</div>
            <div class="tooltip-count">Total: ${data.count} entries</div>
            <div class="tooltip-types">${typeBreakdown}</div>
        `)
        .style('left', `${event.pageX + 10}px`)
        .style('top', `${event.pageY + 10}px`)
        .classed('visible', true);
    }

    hideTooltip() {
        d3.select('#timeline-tooltip').classed('visible', false);
    }

    showBarDetails(event, data) {
        window.dispatchEvent(new CustomEvent('timelineBarSelected', {
            detail: {
                date: data.date,
                nodes: data.nodes,
                count: data.count,
                types: data.types
            }
        }));
    }

    handleResize() {
        const rect = this.container.node().getBoundingClientRect();
        this.svg
            .attr('width', rect.width)
            .attr('height', rect.height);

        this.update();
    }

    reset() {
        this.selectedAgents.clear();
        this.selectedTypes = new Set(['genesis', 'entry', 'action', 'link', 'agent', 'create', 'update', 'delete']);
        this.dateRange = null;
        this.currentZoomLevel = 'day';
        this.update();
    }
}

window.TimelineView = TimelineView;
