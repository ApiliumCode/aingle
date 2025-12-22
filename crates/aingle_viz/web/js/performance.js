/**
 * Performance Optimizations for DAG Visualization
 * Includes virtualization, lazy loading, and optional WebGL rendering
 */

class PerformanceOptimizer {
    constructor(graph) {
        this.graph = graph;
        this.virtualizationEnabled = false;
        this.lazyLoadingEnabled = true;
        this.webglEnabled = false;
        this.visibleNodes = new Set();
        this.viewport = { x: 0, y: 0, width: 0, height: 0, scale: 1 };
        this.nodeThreshold = 1000; // Switch to optimization mode when exceeding this
        this.renderQueue = [];
        this.isRendering = false;

        this.init();
    }

    init() {
        this.detectCapabilities();
        this.setupViewportTracking();
        this.setupRenderQueue();
    }

    /**
     * Detect browser capabilities
     */
    detectCapabilities() {
        // Check WebGL support
        const canvas = document.createElement('canvas');
        const gl = canvas.getContext('webgl') || canvas.getContext('experimental-webgl');
        this.webglSupported = !!gl;

        // Check for IntersectionObserver
        this.intersectionObserverSupported = 'IntersectionObserver' in window;

        // Check for requestIdleCallback
        this.idleCallbackSupported = 'requestIdleCallback' in window;

        console.log('Performance capabilities:', {
            webgl: this.webglSupported,
            intersectionObserver: this.intersectionObserverSupported,
            idleCallback: this.idleCallbackSupported
        });
    }

    /**
     * Track viewport for virtualization
     */
    setupViewportTracking() {
        if (!this.graph || !this.graph.svg) return;

        const svg = this.graph.svg;
        const updateViewport = () => {
            const transform = d3.zoomTransform(svg.node());
            const bounds = svg.node().getBoundingClientRect();

            this.viewport = {
                x: -transform.x / transform.k,
                y: -transform.y / transform.k,
                width: bounds.width / transform.k,
                height: bounds.height / transform.k,
                scale: transform.k
            };

            if (this.virtualizationEnabled) {
                this.updateVisibleNodes();
            }
        };

        // Update on zoom/pan
        if (this.graph.zoom) {
            this.graph.zoom.on('zoom.performance', updateViewport);
        }

        // Initial update
        updateViewport();
    }

    /**
     * Setup render queue for batched updates
     */
    setupRenderQueue() {
        this.processQueue = () => {
            if (this.renderQueue.length === 0) {
                this.isRendering = false;
                return;
            }

            const batch = this.renderQueue.splice(0, 50); // Process 50 at a time
            batch.forEach(fn => fn());

            if (this.renderQueue.length > 0) {
                if (this.idleCallbackSupported) {
                    requestIdleCallback(() => this.processQueue());
                } else {
                    requestAnimationFrame(() => this.processQueue());
                }
            } else {
                this.isRendering = false;
            }
        };
    }

    /**
     * Queue a render operation
     */
    queueRender(fn) {
        this.renderQueue.push(fn);

        if (!this.isRendering) {
            this.isRendering = true;
            if (this.idleCallbackSupported) {
                requestIdleCallback(() => this.processQueue());
            } else {
                requestAnimationFrame(() => this.processQueue());
            }
        }
    }

    /**
     * Update visible nodes based on viewport
     */
    updateVisibleNodes() {
        if (!this.graph || !this.graph.nodes) return;

        const { x, y, width, height } = this.viewport;
        const buffer = 200; // Extra buffer around viewport

        const newVisibleNodes = new Set();

        this.graph.nodes.forEach(node => {
            if (!node.x || !node.y) return;

            const inViewport = (
                node.x >= x - buffer &&
                node.x <= x + width + buffer &&
                node.y >= y - buffer &&
                node.y <= y + height + buffer
            );

            if (inViewport) {
                newVisibleNodes.add(node.id);
            }
        });

        // Update visibility
        const nodesToShow = [...newVisibleNodes].filter(id => !this.visibleNodes.has(id));
        const nodesToHide = [...this.visibleNodes].filter(id => !newVisibleNodes.has(id));

        nodesToShow.forEach(id => {
            this.queueRender(() => {
                d3.select(`#node-${id}`).style('display', 'block');
            });
        });

        nodesToHide.forEach(id => {
            this.queueRender(() => {
                d3.select(`#node-${id}`).style('display', 'none');
            });
        });

        this.visibleNodes = newVisibleNodes;
    }

    /**
     * Enable virtualization for large graphs
     */
    enableVirtualization() {
        this.virtualizationEnabled = true;
        this.updateVisibleNodes();
        console.log('Virtualization enabled');
    }

    /**
     * Disable virtualization
     */
    disableVirtualization() {
        this.virtualizationEnabled = false;

        // Show all nodes
        if (this.graph && this.graph.nodesGroup) {
            this.graph.nodesGroup.selectAll('.node').style('display', 'block');
        }

        console.log('Virtualization disabled');
    }

    /**
     * Optimize graph rendering based on node count
     */
    optimize() {
        if (!this.graph || !this.graph.nodes) return;

        const nodeCount = this.graph.nodes.length;

        if (nodeCount > this.nodeThreshold) {
            // Enable optimizations for large graphs
            if (!this.virtualizationEnabled) {
                this.enableVirtualization();
            }

            // Reduce simulation strength
            if (this.graph.simulation) {
                this.graph.simulation
                    .force('charge', d3.forceManyBody().strength(-30))
                    .alphaDecay(0.05);
            }

            // Simplify rendering
            this.simplifyRendering();

        } else {
            // Disable optimizations for small graphs
            if (this.virtualizationEnabled) {
                this.disableVirtualization();
            }
        }
    }

    /**
     * Simplify rendering for performance
     */
    simplifyRendering() {
        if (!this.graph) return;

        // Hide labels on small nodes
        if (this.graph.nodesGroup) {
            this.graph.nodesGroup.selectAll('text')
                .style('display', d => this.getNodeRadius(d) < 5 ? 'none' : 'block');
        }

        // Reduce edge opacity
        if (this.graph.edgesGroup) {
            this.graph.edgesGroup.selectAll('line')
                .attr('stroke-opacity', 0.2);
        }
    }

    /**
     * Get node radius (fallback if graph method not available)
     */
    getNodeRadius(node) {
        if (this.graph && this.graph.getNodeRadius) {
            return this.graph.getNodeRadius(node);
        }
        return 5; // default
    }

    /**
     * Lazy load node details
     */
    async lazyLoadNodeDetails(nodeId) {
        if (!this.lazyLoadingEnabled) {
            return await this.fetchNodeDetails(nodeId);
        }

        // Check cache
        const cacheKey = `node-details-${nodeId}`;
        const cached = sessionStorage.getItem(cacheKey);

        if (cached) {
            return JSON.parse(cached);
        }

        // Fetch and cache
        const details = await this.fetchNodeDetails(nodeId);
        sessionStorage.setItem(cacheKey, JSON.stringify(details));

        return details;
    }

    /**
     * Fetch node details from API
     */
    async fetchNodeDetails(nodeId) {
        try {
            const response = await fetch(`/api/dag/entry/${nodeId}`);
            if (!response.ok) {
                throw new Error('Failed to fetch node details');
            }
            return await response.json();
        } catch (error) {
            console.error('Error fetching node details:', error);
            return null;
        }
    }

    /**
     * Debounce function for performance
     */
    debounce(func, wait) {
        let timeout;
        return function executedFunction(...args) {
            const later = () => {
                clearTimeout(timeout);
                func(...args);
            };
            clearTimeout(timeout);
            timeout = setTimeout(later, wait);
        };
    }

    /**
     * Throttle function for performance
     */
    throttle(func, limit) {
        let inThrottle;
        return function(...args) {
            if (!inThrottle) {
                func.apply(this, args);
                inThrottle = true;
                setTimeout(() => inThrottle = false, limit);
            }
        };
    }

    /**
     * Batch DOM updates
     */
    batchUpdate(updates) {
        requestAnimationFrame(() => {
            updates.forEach(update => update());
        });
    }

    /**
     * Enable/disable lazy loading
     */
    setLazyLoading(enabled) {
        this.lazyLoadingEnabled = enabled;
        console.log(`Lazy loading ${enabled ? 'enabled' : 'disabled'}`);
    }

    /**
     * Clear caches
     */
    clearCache() {
        // Clear session storage cache
        const keys = Object.keys(sessionStorage);
        keys.forEach(key => {
            if (key.startsWith('node-details-')) {
                sessionStorage.removeItem(key);
            }
        });
        console.log('Cache cleared');
    }

    /**
     * Get performance stats
     */
    getStats() {
        return {
            virtualizationEnabled: this.virtualizationEnabled,
            lazyLoadingEnabled: this.lazyLoadingEnabled,
            webglEnabled: this.webglEnabled,
            webglSupported: this.webglSupported,
            visibleNodeCount: this.visibleNodes.size,
            totalNodeCount: this.graph ? this.graph.nodes.length : 0,
            renderQueueLength: this.renderQueue.length,
            viewport: this.viewport
        };
    }

    /**
     * Auto-optimize based on performance metrics
     */
    autoOptimize() {
        // Measure frame rate
        let lastTime = performance.now();
        let frameCount = 0;
        let fps = 60;

        const measureFPS = () => {
            frameCount++;
            const currentTime = performance.now();
            const elapsed = currentTime - lastTime;

            if (elapsed >= 1000) {
                fps = Math.round((frameCount * 1000) / elapsed);
                frameCount = 0;
                lastTime = currentTime;

                // Auto-optimize if FPS is low
                if (fps < 30) {
                    console.warn(`Low FPS detected: ${fps}. Enabling optimizations...`);
                    this.enableVirtualization();
                } else if (fps > 55 && this.virtualizationEnabled) {
                    console.log(`Good FPS: ${fps}. Disabling aggressive optimizations...`);
                    this.disableVirtualization();
                }
            }

            requestAnimationFrame(measureFPS);
        };

        measureFPS();
    }

    /**
     * Memory management
     */
    cleanupMemory() {
        // Clear render queue
        this.renderQueue = [];

        // Clear visible nodes set if too large
        if (this.visibleNodes.size > 10000) {
            this.visibleNodes.clear();
        }

        // Force garbage collection hint (not guaranteed)
        if (window.gc) {
            window.gc();
        }
    }

    /**
     * Progressive loading for initial data
     */
    async progressiveLoad(nodes, batchSize = 100) {
        const batches = [];
        for (let i = 0; i < nodes.length; i += batchSize) {
            batches.push(nodes.slice(i, i + batchSize));
        }

        for (const batch of batches) {
            await new Promise(resolve => {
                requestAnimationFrame(() => {
                    // Add batch to graph
                    if (this.graph && this.graph.addNode) {
                        batch.forEach(node => this.graph.addNode(node));
                    }
                    resolve();
                });
            });
        }
    }

    /**
     * Enable WebGL rendering (experimental)
     */
    enableWebGL() {
        if (!this.webglSupported) {
            console.warn('WebGL is not supported in this browser');
            return false;
        }

        // WebGL implementation would go here
        // This is a placeholder for future implementation
        console.log('WebGL rendering is not yet implemented');
        this.webglEnabled = false;
        return false;
    }

    /**
     * Disable WebGL rendering
     */
    disableWebGL() {
        this.webglEnabled = false;
        console.log('WebGL rendering disabled');
    }
}

window.PerformanceOptimizer = PerformanceOptimizer;
