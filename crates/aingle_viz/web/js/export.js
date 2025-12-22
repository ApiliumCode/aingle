/**
 * Export Functionality for DAG Visualization
 * Supports SVG, PNG, and JSON export formats
 */

class ExportManager {
    constructor() {
        this.init();
    }

    init() {
        this.setupExportButtons();
    }

    setupExportButtons() {
        const svgBtn = document.getElementById('export-svg');
        const pngBtn = document.getElementById('export-png');
        const jsonBtn = document.getElementById('export-json');
        const csvBtn = document.getElementById('export-csv');

        if (svgBtn) svgBtn.addEventListener('click', () => this.exportSVG());
        if (pngBtn) pngBtn.addEventListener('click', () => this.exportPNG());
        if (jsonBtn) jsonBtn.addEventListener('click', () => this.exportJSON());
        if (csvBtn) csvBtn.addEventListener('click', () => this.exportCSV());
    }

    /**
     * Export DAG as SVG
     */
    async exportSVG(filename = null) {
        try {
            const svgElement = document.querySelector('#graph') || document.querySelector('#dag-graph');
            if (!svgElement) {
                throw new Error('SVG element not found');
            }

            // Clone the SVG to avoid modifying the original
            const clonedSvg = svgElement.cloneNode(true);

            // Add XML declaration and styling
            const serializer = new XMLSerializer();
            let svgString = serializer.serializeToString(clonedSvg);

            // Add CSS styles inline
            const styleString = this.extractInlineStyles();
            svgString = svgString.replace('<svg', `<svg xmlns="http://www.w3.org/2000/svg">`);
            svgString = `<?xml version="1.0" standalone="no"?>
<!DOCTYPE svg PUBLIC "-/W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<style>
${styleString}
</style>
${svgString}`;

            // Create download
            const blob = new Blob([svgString], { type: 'image/svg+xml;charset=utf-8' });
            this.downloadBlob(blob, filename || this.generateFilename('svg'));

            this.showNotification('SVG exported successfully', 'success');
        } catch (error) {
            console.error('Export SVG error:', error);
            this.showNotification('Failed to export SVG', 'error');
        }
    }

    /**
     * Export DAG as PNG
     */
    async exportPNG(filename = null, scale = 2) {
        try {
            const svgElement = document.querySelector('#graph') || document.querySelector('#dag-graph');
            if (!svgElement) {
                throw new Error('SVG element not found');
            }

            // Get SVG dimensions
            const bbox = svgElement.getBoundingClientRect();
            const width = bbox.width * scale;
            const height = bbox.height * scale;

            // Create canvas
            const canvas = document.createElement('canvas');
            canvas.width = width;
            canvas.height = height;
            const ctx = canvas.getContext('2d');

            // Set background color
            ctx.fillStyle = '#1a1a2e';
            ctx.fillRect(0, 0, width, height);

            // Convert SVG to data URL
            const serializer = new XMLSerializer();
            let svgString = serializer.serializeToString(svgElement);

            // Add inline styles
            const styleString = this.extractInlineStyles();
            svgString = svgString.replace('<svg', `<svg xmlns="http://www.w3.org/2000/svg" style="${styleString}"`);

            const svgBlob = new Blob([svgString], { type: 'image/svg+xml;charset=utf-8' });
            const url = URL.createObjectURL(svgBlob);

            // Load image and draw to canvas
            const img = new Image();
            img.onload = () => {
                ctx.drawImage(img, 0, 0, width, height);
                URL.revokeObjectURL(url);

                // Export canvas as PNG
                canvas.toBlob((blob) => {
                    this.downloadBlob(blob, filename || this.generateFilename('png'));
                    this.showNotification('PNG exported successfully', 'success');
                }, 'image/png');
            };

            img.onerror = () => {
                URL.revokeObjectURL(url);
                throw new Error('Failed to load SVG image');
            };

            img.src = url;

        } catch (error) {
            console.error('Export PNG error:', error);
            this.showNotification('Failed to export PNG', 'error');
        }
    }

    /**
     * Export DAG data as JSON
     */
    async exportJSON(filename = null, includeMetadata = true) {
        try {
            // Get DAG data from the app
            const data = await this.fetchDagData();

            if (includeMetadata) {
                data.metadata = {
                    exportDate: new Date().toISOString(),
                    version: '1.0',
                    application: 'AIngle DAG Visualization',
                    nodeCount: data.nodes.length,
                    edgeCount: data.edges.length
                };
            }

            const jsonString = JSON.stringify(data, null, 2);
            const blob = new Blob([jsonString], { type: 'application/json' });
            this.downloadBlob(blob, filename || this.generateFilename('json'));

            this.showNotification('JSON exported successfully', 'success');
        } catch (error) {
            console.error('Export JSON error:', error);
            this.showNotification('Failed to export JSON', 'error');
        }
    }

    /**
     * Export DAG data as CSV
     */
    async exportCSV(filename = null) {
        try {
            const data = await this.fetchDagData();

            // Convert nodes to CSV
            const headers = ['id', 'label', 'type', 'agent_id', 'timestamp', 'parents'];
            const rows = data.nodes.map(node => [
                this.escapeCsv(node.id),
                this.escapeCsv(node.label),
                this.escapeCsv(node.node_type || node.group),
                this.escapeCsv(node.agent_id || node.author || ''),
                node.timestamp || '',
                this.escapeCsv((node.parents || []).join(';'))
            ]);

            const csv = [
                headers.join(','),
                ...rows.map(row => row.join(','))
            ].join('\n');

            const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
            this.downloadBlob(blob, filename || this.generateFilename('csv'));

            this.showNotification('CSV exported successfully', 'success');
        } catch (error) {
            console.error('Export CSV error:', error);
            this.showNotification('Failed to export CSV', 'error');
        }
    }

    /**
     * Export filtered data
     */
    async exportFiltered(format = 'json', filterFn = null) {
        try {
            let data = await this.fetchDagData();

            if (filterFn) {
                data.nodes = data.nodes.filter(filterFn);
                // Update edges to only include those connected to filtered nodes
                const nodeIds = new Set(data.nodes.map(n => n.id));
                data.edges = data.edges.filter(e =>
                    nodeIds.has(e.source.id || e.source) &&
                    nodeIds.has(e.target.id || e.target)
                );
            }

            switch (format) {
                case 'json':
                    await this.exportJSON('filtered-dag.json', true);
                    break;
                case 'csv':
                    await this.exportCSV('filtered-dag.csv');
                    break;
                case 'svg':
                    await this.exportSVG('filtered-dag.svg');
                    break;
                case 'png':
                    await this.exportPNG('filtered-dag.png');
                    break;
                default:
                    throw new Error(`Unsupported format: ${format}`);
            }
        } catch (error) {
            console.error('Export filtered error:', error);
            this.showNotification('Failed to export filtered data', 'error');
        }
    }

    /**
     * Helper: Fetch DAG data from API or graph instance
     */
    async fetchDagData() {
        // Try to get from window.app.graph first
        if (window.app && window.app.graph) {
            return {
                nodes: window.app.graph.nodes || [],
                edges: window.app.graph.edges || []
            };
        }

        // Fallback to API
        const response = await fetch('/api/dag/d3');
        if (!response.ok) {
            throw new Error('Failed to fetch DAG data');
        }
        return await response.json();
    }

    /**
     * Helper: Extract inline styles from CSS
     */
    extractInlineStyles() {
        const styles = `
            .node circle {
                stroke: #fff;
                stroke-width: 2px;
            }
            .node text {
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
                font-size: 10px;
                fill: #eee;
            }
            .link {
                stroke: #666;
                stroke-opacity: 0.6;
                fill: none;
            }
            .link-label {
                font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, sans-serif;
                font-size: 8px;
                fill: #888;
            }
        `;
        return styles;
    }

    /**
     * Helper: Generate filename with timestamp
     */
    generateFilename(extension) {
        const timestamp = new Date().toISOString().replace(/:/g, '-').split('.')[0];
        return `aingle-dag-${timestamp}.${extension}`;
    }

    /**
     * Helper: Download blob as file
     */
    downloadBlob(blob, filename) {
        const url = URL.createObjectURL(blob);
        const link = document.createElement('a');
        link.href = url;
        link.download = filename;
        document.body.appendChild(link);
        link.click();
        document.body.removeChild(link);
        URL.revokeObjectURL(url);
    }

    /**
     * Helper: Escape CSV values
     */
    escapeCsv(value) {
        if (value === null || value === undefined) return '';
        const str = String(value);
        if (str.includes(',') || str.includes('"') || str.includes('\n')) {
            return `"${str.replace(/"/g, '""')}"`;
        }
        return str;
    }

    /**
     * Helper: Show notification
     */
    showNotification(message, type = 'info') {
        if (window.notificationManager) {
            window.notificationManager.show(message, type);
        } else {
            console.log(`[${type.toUpperCase()}] ${message}`);
        }
    }

    /**
     * Batch export all formats
     */
    async exportAll() {
        try {
            this.showNotification('Exporting all formats...', 'info');

            await this.exportJSON();
            await new Promise(resolve => setTimeout(resolve, 500));

            await this.exportCSV();
            await new Promise(resolve => setTimeout(resolve, 500));

            await this.exportSVG();
            await new Promise(resolve => setTimeout(resolve, 500));

            await this.exportPNG();

            this.showNotification('All formats exported successfully', 'success');
        } catch (error) {
            console.error('Export all error:', error);
            this.showNotification('Failed to export all formats', 'error');
        }
    }

    /**
     * Export current view state (zoom, pan, filters)
     */
    exportViewState() {
        try {
            const state = {
                timestamp: new Date().toISOString(),
                zoom: null,
                pan: null,
                filters: null
            };

            // Get zoom/pan from D3
            const svgElement = document.querySelector('#graph') || document.querySelector('#dag-graph');
            if (svgElement) {
                const transform = d3.select(svgElement).select('g').attr('transform');
                if (transform) {
                    const match = transform.match(/translate\(([^,]+),([^)]+)\).*scale\(([^)]+)\)/);
                    if (match) {
                        state.pan = { x: parseFloat(match[1]), y: parseFloat(match[2]) };
                        state.zoom = parseFloat(match[3]);
                    }
                }
            }

            // Get filter state
            if (window.filterManager) {
                state.filters = window.filterManager.getState();
            }

            const jsonString = JSON.stringify(state, null, 2);
            const blob = new Blob([jsonString], { type: 'application/json' });
            this.downloadBlob(blob, this.generateFilename('view-state.json'));

            this.showNotification('View state exported', 'success');
        } catch (error) {
            console.error('Export view state error:', error);
            this.showNotification('Failed to export view state', 'error');
        }
    }
}

window.ExportManager = ExportManager;
