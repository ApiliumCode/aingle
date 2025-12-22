# Integration Guide for AIngle DAG Visualization

This guide explains how to integrate the enhanced DAG visualization into your application.

## Quick Integration Steps

### Option 1: Use Enhanced Interface (Recommended)

The enhanced interface (`index-enhanced.html`) includes all new features out of the box.

1. **Update API serving** to use the enhanced interface:

```rust
// In src/api.rs
async fn serve_index() -> Html<&'static str> {
    Html(include_str!("../web/index-enhanced.html"))
}
```

2. **Ensure all JS files are served**:

The router already serves static files, but verify these paths work:
- `/js/timeline.js`
- `/js/filters.js`
- `/js/export.js`
- `/js/notifications.js`
- `/js/performance.js`

3. **Update CSS link** in HTML to use external stylesheet:

```html
<link rel="stylesheet" href="/css/style.css">
```

### Option 2: Gradual Integration

Add features one at a time to your existing interface.

#### Add Timeline View

1. Include the script:
```html
<script src="/js/timeline.js"></script>
```

2. Add container:
```html
<svg id="timeline-graph" style="display: none;"></svg>
```

3. Initialize:
```javascript
const timeline = new TimelineView('timeline-graph');
timeline.setData(nodes);
```

#### Add Advanced Filters

1. Include the script:
```html
<script src="/js/filters.js"></script>
```

2. Add filter UI elements:
```html
<div id="filter-types"></div>
<div id="filter-agents"></div>
<input type="search" id="filter-search">
```

3. Initialize:
```javascript
const filterManager = new FilterManager();
filterManager.onChange((state, manager) => {
    const filtered = manager.apply(graph.nodes);
    // Update visualizations
});
```

#### Add Export Functionality

1. Include the script:
```html
<script src="/js/export.js"></script>
```

2. Add export buttons:
```html
<button id="export-svg">Export SVG</button>
<button id="export-png">Export PNG</button>
<button id="export-json">Export JSON</button>
<button id="export-csv">Export CSV</button>
```

3. Initialize:
```javascript
const exportManager = new ExportManager();
// Export buttons are auto-wired
```

#### Add Notifications

1. Include the script:
```html
<script src="/js/notifications.js"></script>
```

2. Initialize:
```javascript
const notificationManager = new NotificationManager();
window.notificationManager = notificationManager; // Make global

// Use in your code
notificationManager.success('Connected!');
```

#### Add Performance Optimizations

1. Include the script:
```html
<script src="/js/performance.js"></script>
```

2. Initialize:
```javascript
const optimizer = new PerformanceOptimizer(graph);
optimizer.autoOptimize();
```

## API Integration

### Serving Static Files

The current implementation uses `include_str!` and `include_bytes!` to embed files. For production, you might want to serve them from disk:

```rust
use tower_http::services::ServeDir;

pub fn create_router(state: ApiState) -> Router {
    Router::new()
        // API routes
        .route("/api/dag", get(get_dag))
        // ... other API routes

        // Serve static files from disk
        .nest_service("/js", ServeDir::new("web/js"))
        .nest_service("/css", ServeDir::new("web/css"))
        .nest_service("/assets", ServeDir::new("web/assets"))

        // Serve index.html
        .route("/", get(serve_index))
        .with_state(state)
}
```

### WebSocket Integration

The WebSocket handler is already integrated. To add custom events:

```rust
// In src/events.rs
pub enum DagEvent {
    // ... existing events
    CustomEvent {
        data: serde_json::Value,
    },
}

impl DagEvent {
    pub fn custom(data: serde_json::Value) -> Self {
        Self::CustomEvent { data }
    }
}

// Broadcast custom events
broadcaster.broadcast(DagEvent::custom(json!({
    "message": "Custom event"
}))).await;
```

### Adding Custom API Endpoints

```rust
// In src/api.rs
async fn custom_endpoint(
    State(state): State<ApiState>,
) -> Json<serde_json::Value> {
    // Your custom logic
    Json(json!({
        "status": "ok"
    }))
}

// In create_router()
.route("/api/custom", get(custom_endpoint))
```

## Frontend Customization

### Custom Color Schemes

Modify CSS variables in `style.css`:

```css
:root {
    --bg-primary: #1a1a2e;
    --bg-secondary: #16213e;
    --bg-tertiary: #0f3460;
    --text-primary: #eee;
    --text-secondary: #aaa;
    --accent-blue: #4fc3f7;
    --accent-green: #66bb6a;
    --accent-orange: #ffa726;
    --accent-red: #ef5350;
    --accent-purple: #ab47bc;
    --border-color: #333;
}
```

### Custom Node Types

Add new node types in both backend and frontend:

**Backend** (`src/dag.rs`):
```rust
pub enum NodeType {
    // ... existing types
    CustomType,
}
```

**Frontend** (`js/dag-graph.js`):
```javascript
this.typeColors = {
    // ... existing colors
    customtype: '#YOUR_COLOR',
};
```

**Frontend** (`js/filters.js`):
```javascript
const types = [
    // ... existing types
    { id: 'customtype', label: 'Custom Type', color: '#YOUR_COLOR' },
];
```

### Custom Filters

Add custom filter logic:

```javascript
filterManager.addCustomFilter('myFilter', (node) => {
    // Return true to include node, false to exclude
    return node.myProperty === 'myValue';
});
```

### Custom Export Formats

Extend the ExportManager:

```javascript
ExportManager.prototype.exportCustomFormat = async function() {
    const data = await this.fetchDagData();

    // Your custom export logic
    const customData = transformData(data);

    const blob = new Blob([customData], { type: 'application/custom' });
    this.downloadBlob(blob, 'export.custom');
};
```

## Testing Your Integration

### 1. Run Tests

```bash
# Run all tests
cargo test -p aingle_viz

# Run specific test
cargo test -p aingle_viz test_get_dag_endpoint
```

### 2. Manual Testing Checklist

- [ ] DAG visualization loads and renders
- [ ] WebSocket connection establishes
- [ ] Nodes can be selected and details shown
- [ ] Zoom and pan work correctly
- [ ] Timeline view toggles properly
- [ ] Filters apply to both DAG and timeline
- [ ] All export formats work
- [ ] Notifications appear and dismiss
- [ ] Mobile sidebar toggles
- [ ] Touch gestures work on mobile
- [ ] Performance is acceptable (FPS > 30)

### 3. Browser Testing

Test in these browsers:
- Chrome/Edge (latest)
- Firefox (latest)
- Safari (latest)
- Mobile Safari (iOS)
- Mobile Chrome (Android)

### 4. Performance Testing

```javascript
// Check performance stats
console.log(window.app.performanceOptimizer.getStats());

// Monitor FPS
// Auto-optimization will log warnings if FPS < 30

// Check memory usage
// Open Chrome DevTools > Memory > Take snapshot
```

## Common Integration Issues

### Issue: Static files not loading

**Solution**: Verify file paths in `create_router()`:
```rust
.nest_service("/js", ServeDir::new("web/js"))
```

### Issue: WebSocket connection fails

**Solution**: Check WebSocket URL protocol matches HTTP:
```javascript
const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
```

### Issue: Notifications not showing

**Solution**: Ensure NotificationManager is initialized first:
```javascript
notificationManager = new NotificationManager();
window.notificationManager = notificationManager;
```

### Issue: Filters not working

**Solution**: Verify filter UI elements exist:
```javascript
// Check if elements are present
console.log(document.getElementById('filter-types'));
console.log(document.getElementById('filter-agents'));
```

### Issue: Export fails

**Solution**: Check if graph data is available:
```javascript
console.log(window.app.graph.nodes.length);
console.log(window.app.graph.edges.length);
```

## Deployment Considerations

### Production Build

```bash
# Build with optimizations
cargo build --release --bin aingle_viz

# Binary will be at:
# target/release/aingle_viz
```

### Environment Variables

```bash
export VIZ_HOST=0.0.0.0  # Allow external connections
export VIZ_PORT=8080     # Port number
```

### HTTPS Setup

For production, use a reverse proxy (nginx, Caddy) to handle HTTPS:

```nginx
server {
    listen 443 ssl;
    server_name your-domain.com;

    ssl_certificate /path/to/cert.pem;
    ssl_certificate_key /path/to/key.pem;

    location / {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }
}
```

### Docker Deployment

Create a `Dockerfile`:

```dockerfile
FROM rust:1.75 as builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin aingle_viz

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/aingle_viz /usr/local/bin/
EXPOSE 8080
CMD ["aingle_viz"]
```

Build and run:
```bash
docker build -t aingle-viz .
docker run -p 8080:8080 aingle-viz
```

## Support and Troubleshooting

### Enable Debug Logging

```rust
// In main.rs
env_logger::Builder::from_env(
    env_logger::Env::default().default_filter_or("debug")
).init();
```

### Frontend Debugging

```javascript
// Enable verbose logging
window.DEBUG = true;

// Check app state
console.log(window.app);

// Check graph state
console.log(window.app.graph.nodes);
console.log(window.app.graph.edges);

// Check filter state
console.log(window.app.filterManager.getState());

// Check performance stats
console.log(window.app.performanceOptimizer.getStats());
```

### WebSocket Debugging

```javascript
// Monitor WebSocket messages
const originalOnMessage = ws.onmessage;
ws.onmessage = (event) => {
    console.log('WS received:', event.data);
    originalOnMessage(event);
};
```

## Next Steps

1. Choose integration option (Enhanced or Gradual)
2. Update your API routing
3. Test in development
4. Customize as needed
5. Run full test suite
6. Deploy to production

For more information, see:
- [README.md](README.md) - General documentation
- [FEATURES.md](FEATURES.md) - Detailed feature list
- [src/api.rs](src/api.rs) - API implementation
- [web/index-enhanced.html](web/index-enhanced.html) - Complete example

---

**Questions?** Open an issue on GitHub or contact the development team.
