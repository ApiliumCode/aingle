# Tutoriales de AIngle

Bienvenido a la colección de tutoriales de AIngle. Estos tutoriales te guiarán desde los conceptos básicos hasta casos de uso avanzados.

> **Available in English:** [English Tutorials](../)

## Tutoriales principales (Recomendado seguir en orden)

### 1. [Inicio Rápido](./getting-started.md)
**Tiempo:** 30-45 minutos | **Nivel:** Principiante

Aprende los fundamentos de AIngle:
- Instalación y configuración
- Crear tu primer nodo
- Conectar a la red
- Crear y consultar entradas en el DAG
- Troubleshooting básico

**Empieza aquí si eres nuevo en AIngle.**

---

### 2. [Red de Sensores IoT](./iot-sensor-network.md)
**Tiempo:** 60-90 minutos | **Nivel:** Intermedio

Construye una red de sensores IoT:
- Configurar nodo minimal optimizado para IoT
- Conectar sensores de temperatura y humedad
- Protocolo CoAP para dispositivos con recursos limitados
- Gossip protocol para sincronización entre dispositivos
- Dashboard de visualización en tiempo real

**Ideal para:** Proyectos IoT, edge computing, redes de sensores

---

### 3. [Aplicación con IA usando HOPE Agents](./ai-powered-app.md)
**Tiempo:** 90-120 minutos | **Nivel:** Avanzado

Añade inteligencia artificial a tus aplicaciones:
- Configurar y entrenar HOPE Agents
- Q-Learning y aprendizaje por refuerzo
- Metas jerárquicas y planificación
- Detección automática de anomalías
- Toma de decisiones autónoma

**Ideal para:** Aplicaciones inteligentes, automatización, sistemas adaptativos

---

### 4. [Consultas Semánticas con Córtex](./semantic-queries.md)
**Tiempo:** 75-90 minutos | **Nivel:** Intermedio

Consulta datos con APIs avanzadas:
- API REST de Córtex
- Consultas flexibles con GraphQL
- Búsquedas semánticas con SPARQL
- Filtros avanzados y agregaciones
- Subscripciones en tiempo real con WebSocket

**Ideal para:** Análisis de datos, búsquedas complejas, integración con frontends

---

### 5. [Privacidad con Zero-Knowledge Proofs](./privacy-with-zk.md)
**Tiempo:** 60-75 minutos | **Nivel:** Intermedio-Avanzado

Protege datos sensibles con criptografía:
- Hash y Pedersen Commitments
- Schnorr proofs (prueba de conocimiento)
- Range proofs (probar rango sin revelar valor)
- Verificación batch para eficiencia
- Casos de uso: votación privada, transacciones confidenciales

**Ideal para:** Privacidad de datos, compliance, aplicaciones financieras

---

### 6. [Visualización del DAG](./dag-visualization.md)
**Tiempo:** 45-60 minutos | **Nivel:** Principiante-Intermedio

Visualiza el grafo en tiempo real:
- Iniciar servidor de visualización
- Navegar el grafo interactivamente
- Filtros y búsqueda de nodos
- Export a JSON, GraphML, CSV
- Personalización de colores y layouts

**Ideal para:** Debugging, análisis de red, presentaciones

---

## Tutoriales adicionales

### [Aplicación IoT con Sensores](./iot-sensor-app.md)
Tutorial anterior sobre IoT (más básico que iot-sensor-network.md)

### [Grafo Semántico](./semantic-graph.md)
Trabajo con grafos semánticos y RDF

### [Smart Contracts](./smart-contracts.md)
Contratos inteligentes en AIngle

---

## Rutas de aprendizaje recomendadas

### Para desarrolladores IoT:
1. [Inicio Rápido](./getting-started.md)
2. [Red de Sensores IoT](./iot-sensor-network.md)
3. [Visualización del DAG](./dag-visualization.md)
4. [Privacidad con ZK](./privacy-with-zk.md) (opcional)

### Para desarrolladores de IA/ML:
1. [Inicio Rápido](./getting-started.md)
2. [Aplicación con IA](./ai-powered-app.md)
3. [Consultas Semánticas](./semantic-queries.md)
4. [Visualización del DAG](./dag-visualization.md)

### Para desarrolladores web/APIs:
1. [Inicio Rápido](./getting-started.md)
2. [Consultas Semánticas](./semantic-queries.md)
3. [Visualización del DAG](./dag-visualization.md)
4. [Red de Sensores IoT](./iot-sensor-network.md) (opcional)

### Para desarrolladores blockchain/DeFi:
1. [Inicio Rápido](./getting-started.md)
2. [Privacidad con ZK](./privacy-with-zk.md)
3. [Smart Contracts](./smart-contracts.md)
4. [Consultas Semánticas](./semantic-queries.md)

---

## Conceptos clave por tutorial

| Tutorial | Conceptos principales |
|----------|----------------------|
| Getting Started | Nodos, DAG, Entries, Hash, mDNS, Gossip |
| IoT Sensor Network | CoAP, Minimal node, Power modes, Batch readings |
| AI-Powered App | HOPE Agents, Q-Learning, Hierarchical goals, Anomaly detection |
| Semantic Queries | REST API, GraphQL, SPARQL, WebSocket, Filtering |
| Privacy with ZK | Commitments, Range proofs, Schnorr proofs, Batch verification |
| DAG Visualization | D3.js, Force-directed layout, Graph export, Real-time updates |

---

## Prerrequisitos generales

### Software requerido:
- **Rust**: 1.70 o superior
- **Cargo**: Gestor de paquetes de Rust
- **Git**: Para clonar el repositorio
- **Navegador moderno**: Chrome, Firefox, o Safari (para visualización)

### Conocimientos recomendados:
- **Básico**: Rust básico, línea de comandos
- **Intermedio**: Conceptos de redes, APIs REST, JSON
- **Avanzado**: Aprendizaje automático, criptografía, protocolos distribuidos

### Hardware mínimo:
- **RAM**: 2 GB (4 GB recomendado)
- **Disco**: 1 GB de espacio libre
- **CPU**: Cualquier procesador moderno (64-bit)
- **Red**: Conexión a internet (para descargar dependencias)

---

## Instalación

Antes de comenzar cualquier tutorial, instala AIngle:

```bash
# Clonar repositorio
git clone https://github.com/ApiliumCode/aingle.git
cd aingle

# Compilar proyecto
cargo build --release

# Verificar instalación
./target/release/aingle --version
```

---

## Soporte y recursos

### Documentación:
- [Arquitectura](../../architecture/overview.md)
- [API Reference](../../api/README.md)
- [Core Testing](../../core_testing.md)

### Código de ejemplo:
- [Examples Directory](../../../examples/)
- [Templates](../../../templates/)

### Comunidad:
- GitHub Issues: [ApiliumCode/aingle/issues](https://github.com/ApiliumCode/aingle/issues)

---

## Contribuir

¿Encontraste un error en un tutorial o quieres añadir uno nuevo?

1. Fork el repositorio
2. Crea una rama: `git checkout -b tutorial/mi-nuevo-tutorial`
3. Añade tu tutorial en `docs/tutorials/`
4. Actualiza este README.md
5. Crea un Pull Request

**Formato esperado:**
- Markdown con código resaltado
- Secciones numeradas paso a paso
- Resultados esperados claramente marcados
- Troubleshooting común al final
- Referencias y próximos pasos

---

## Licencia

Todos los tutoriales están bajo la misma licencia que el proyecto AIngle.

Ver [LICENSE](../../../LICENSE) para más detalles.
