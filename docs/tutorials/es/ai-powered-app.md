# Tutorial: Aplicación con IA usando HOPE Agents

## Objetivo

Construir una aplicación inteligente que aprende automáticamente y toma decisiones usando HOPE Agents (Hierarchical Optimized Policy Engine). Los agentes pueden aprender de la experiencia, manejar metas jerárquicas, detectar anomalías y ejecutar acciones autónomas.

## Prerrequisitos

- Completar el [tutorial de inicio rápido](./getting-started.md)
- Conocimientos básicos de aprendizaje por refuerzo
- Familiaridad con conceptos de IA (opcional pero útil)

## Tiempo estimado

90-120 minutos

---

## Paso 1: Entender HOPE Agents

HOPE (Hierarchical Optimized Policy Engine) combina:

- **Q-Learning**: Aprende valores de estado-acción
- **SARSA**: Aprende políticas on-policy
- **TD-Learning**: Aprendizaje por diferencia temporal
- **Hierarchical Goals**: Descompone objetivos complejos
- **Predictive Model**: Predice estados futuros
- **Anomaly Detection**: Detecta comportamientos inusuales

**Arquitectura:**

```
┌─────────────────────────────────────────┐
│           HOPE Agent                     │
├─────────────────────────────────────────┤
│  ┌──────────┐  ┌──────────────────┐     │
│  │ Learning │  │ Goal Solver      │     │
│  │ Engine   │  │ (Hierarchical)   │     │
│  └──────────┘  └──────────────────┘     │
│  ┌──────────────────────────────────┐   │
│  │ Predictive Model                  │   │
│  │ - Forecasting                     │   │
│  │ - Anomaly Detection               │   │
│  └──────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

---

## Paso 2: Configurar HOPE Agent básico

Crea un nuevo proyecto:

```bash
mkdir aingle-ai-app
cd aingle-ai-app
cargo init
```

Añade dependencias al `Cargo.toml`:

```toml
[package]
name = "aingle-ai-app"
version = "0.1.0"
edition = "2021"

[dependencies]
hope_agents = { path = "../../crates/hope_agents" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
env_logger = "0.11"
```

Crea tu primer agente:

```rust
// src/main.rs
use hope_agents::{HopeAgent, AgentConfig, Observation, ActionResult};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // Configurar agente
    let config = AgentConfig {
        name: "smart_thermostat".to_string(),
        max_memory_bytes: 2 * 1024 * 1024, // 2 MB
        decision_interval: Duration::from_millis(200),
        max_goals: 20,
        learning_enabled: true,
        learning_rate: 0.15,       // Tasa de aprendizaje
        exploration_rate: 0.2,     // Exploración vs explotación
        max_rules: 500,
        sensor_interval: Duration::from_millis(100),
        action_timeout: Duration::from_secs(10),
    };

    println!("🤖 Creando HOPE Agent: {}", config.name);
    println!("   Learning: {}", config.learning_enabled);
    println!("   Learning rate: {}", config.learning_rate);
    println!("   Exploration: {}%\n", config.exploration_rate * 100.0);

    // Crear agente
    let mut agent = HopeAgent::new(config)?;

    println!("✓ Agente creado y listo para aprender\n");

    Ok(())
}
```

**Explicación de parámetros:**

- `learning_rate (0.15)`: Qué tan rápido aprende (0=nada, 1=instantáneo)
- `exploration_rate (0.2)`: 20% explora acciones nuevas, 80% usa lo aprendido
- `max_goals (20)`: Puede manejar hasta 20 objetivos simultáneos
- `decision_interval`: Toma decisiones cada 200ms

---

## Paso 3: Definir observaciones y acciones

Los agentes aprenden mediante observaciones del entorno y ejecutando acciones:

```rust
// src/thermostat.rs
use serde::{Deserialize, Serialize};
use hope_agents::{Observation, Action, ActionResult};

/// Estado del termostato
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ThermostatState {
    pub current_temp: f64,      // Temperatura actual
    pub target_temp: f64,       // Temperatura deseada
    pub humidity: f64,          // Humedad %
    pub occupancy: bool,        // ¿Hay gente?
    pub time_of_day: u8,        // Hora 0-23
}

impl ThermostatState {
    /// Convertir a observación para el agente
    pub fn to_observation(&self) -> Observation {
        // Discretizar el estado en categorías
        let temp_diff = (self.target_temp - self.current_temp).round() as i32;
        let humidity_level = (self.humidity / 10.0).round() as u8; // 0-10
        let occupancy_flag = if self.occupancy { 1 } else { 0 };

        Observation {
            state_id: format!(
                "temp_{}_hum_{}_occ_{}_hour_{}",
                temp_diff, humidity_level, occupancy_flag, self.time_of_day
            ),
            features: vec![
                self.current_temp,
                self.target_temp,
                self.humidity,
                occupancy_flag as f64,
                self.time_of_day as f64,
            ],
            metadata: Some(serde_json::json!({
                "temp_diff": temp_diff,
                "comfort_level": self.calculate_comfort(),
            })),
        }
    }

    /// Calcular nivel de confort (0-100)
    fn calculate_comfort(&self) -> f64 {
        let temp_comfort = if (self.current_temp - self.target_temp).abs() < 1.0 {
            100.0
        } else {
            let diff = (self.current_temp - self.target_temp).abs();
            (100.0 - diff * 10.0).max(0.0)
        };

        let humidity_comfort = if (40.0..=60.0).contains(&self.humidity) {
            100.0
        } else {
            let diff = if self.humidity < 40.0 {
                40.0 - self.humidity
            } else {
                self.humidity - 60.0
            };
            (100.0 - diff * 2.0).max(0.0)
        };

        (temp_comfort + humidity_comfort) / 2.0
    }
}

/// Acciones que puede tomar el termostato
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ThermostatAction {
    HeatHigh,       // Calentar intenso (+2°C/min)
    HeatLow,        // Calentar suave (+0.5°C/min)
    Cool,           // Enfriar (-1°C/min)
    Fan,            // Solo ventilador (mejorar circulación)
    Off,            // Apagar
}

impl ThermostatAction {
    /// Convertir a Action para el agente
    pub fn to_action(&self) -> Action {
        Action {
            action_id: self.action_name().to_string(),
            parameters: serde_json::to_value(self).unwrap(),
        }
    }

    pub fn action_name(&self) -> &str {
        match self {
            Self::HeatHigh => "heat_high",
            Self::HeatLow => "heat_low",
            Self::Cool => "cool",
            Self::Fan => "fan",
            Self::Off => "off",
        }
    }

    /// Ejecutar acción (simulado)
    pub fn execute(&self, state: &mut ThermostatState) -> ActionResult {
        let initial_comfort = state.calculate_comfort();

        match self {
            Self::HeatHigh => {
                state.current_temp += 2.0;
            }
            Self::HeatLow => {
                state.current_temp += 0.5;
            }
            Self::Cool => {
                state.current_temp -= 1.0;
            }
            Self::Fan => {
                // Mejora circulación, reduce humedad ligeramente
                state.humidity = (state.humidity - 2.0).max(0.0);
            }
            Self::Off => {
                // Deriva hacia temperatura ambiente (20°C)
                let ambient = 20.0;
                let drift = (ambient - state.current_temp) * 0.1;
                state.current_temp += drift;
            }
        }

        let final_comfort = state.calculate_comfort();
        let comfort_improvement = final_comfort - initial_comfort;

        // Recompensa basada en mejorar confort
        let reward = comfort_improvement / 10.0; // Escalar a [-10, 10]

        ActionResult {
            success: true,
            reward,
            new_observation: state.to_observation(),
            done: false,
            metadata: Some(serde_json::json!({
                "comfort_before": initial_comfort,
                "comfort_after": final_comfort,
                "improvement": comfort_improvement,
            })),
        }
    }
}
```

**Explicación:**

- **Observation**: Representa el estado actual del entorno
- **Action**: Representa una acción que el agente puede ejecutar
- **Reward**: Señal de aprendizaje (+recompensa = bueno, -recompensa = malo)
- **calculate_comfort()**: Función objetivo que el agente maximiza

---

## Paso 4: Metas jerárquicas

HOPE Agents soportan metas jerárquicas que se descomponen en submetas:

```rust
// src/goals.rs
use hope_agents::{Goal, GoalPriority};

/// Definir objetivos del termostato
pub fn create_comfort_goals() -> Vec<Goal> {
    vec![
        // Meta principal: Confort óptimo
        Goal {
            goal_id: "optimal_comfort".to_string(),
            description: "Mantener confort óptimo (temp + humedad)".to_string(),
            priority: GoalPriority::High,
            parent_goal: None,
            success_condition: Box::new(|obs| {
                // Éxito si comfort > 90%
                if let Some(metadata) = &obs.metadata {
                    if let Some(comfort) = metadata.get("comfort_level") {
                        return comfort.as_f64().unwrap_or(0.0) > 90.0;
                    }
                }
                false
            }),
            subgoals: vec!["maintain_temp".to_string(), "maintain_humidity".to_string()],
        },
        // Submeta 1: Temperatura
        Goal {
            goal_id: "maintain_temp".to_string(),
            description: "Mantener temperatura objetivo ±1°C".to_string(),
            priority: GoalPriority::Medium,
            parent_goal: Some("optimal_comfort".to_string()),
            success_condition: Box::new(|obs| {
                if let Some(metadata) = &obs.metadata {
                    if let Some(diff) = metadata.get("temp_diff") {
                        let diff_val = diff.as_i64().unwrap_or(10);
                        return diff_val.abs() <= 1;
                    }
                }
                false
            }),
            subgoals: vec![],
        },
        // Submeta 2: Humedad
        Goal {
            goal_id: "maintain_humidity".to_string(),
            description: "Mantener humedad 40-60%".to_string(),
            priority: GoalPriority::Medium,
            parent_goal: Some("optimal_comfort".to_string()),
            success_condition: Box::new(|obs| {
                if obs.features.len() > 2 {
                    let humidity = obs.features[2];
                    return (40.0..=60.0).contains(&humidity);
                }
                false
            }),
            subgoals: vec![],
        },
        // Meta secundaria: Eficiencia energética
        Goal {
            goal_id: "energy_efficiency".to_string(),
            description: "Minimizar uso de energía".to_string(),
            priority: GoalPriority::Low,
            parent_goal: None,
            success_condition: Box::new(|_obs| {
                // Siempre activo, prioridad baja
                false
            }),
            subgoals: vec![],
        },
    ]
}
```

**Explicación:**

- **Hierarchía**: `optimal_comfort` → `maintain_temp` + `maintain_humidity`
- **Prioridades**: High > Medium > Low
- **Success conditions**: Predicados que determinan si se logró el objetivo
- El agente trabaja en múltiples metas simultáneamente

---

## Paso 5: Aprendizaje automático

Entrena el agente con episodios de aprendizaje:

```rust
// src/training.rs
use hope_agents::HopeAgent;
use crate::thermostat::{ThermostatState, ThermostatAction};
use crate::goals::create_comfort_goals;

pub async fn train_agent(agent: &mut HopeAgent, episodes: usize) -> anyhow::Result<()> {
    println!("🎓 Iniciando entrenamiento ({} episodios)...\n", episodes);

    // Cargar objetivos
    let goals = create_comfort_goals();
    for goal in goals {
        agent.add_goal(goal).await?;
    }

    let actions = vec![
        ThermostatAction::HeatHigh,
        ThermostatAction::HeatLow,
        ThermostatAction::Cool,
        ThermostatAction::Fan,
        ThermostatAction::Off,
    ];

    for episode in 0..episodes {
        // Estado inicial aleatorio
        let mut state = ThermostatState {
            current_temp: 15.0 + (rand::random::<f64>() * 10.0), // 15-25°C
            target_temp: 21.0,
            humidity: 40.0 + (rand::random::<f64>() * 30.0), // 40-70%
            occupancy: rand::random(),
            time_of_day: (rand::random::<f64>() * 24.0) as u8,
        };

        let mut total_reward = 0.0;
        let max_steps = 50;

        for step in 0..max_steps {
            // Observar estado actual
            let observation = state.to_observation();

            // Decidir acción (explora o explota)
            let action_name = agent.decide_action(&observation).await?;

            // Ejecutar acción
            let selected_action = actions
                .iter()
                .find(|a| a.action_name() == action_name)
                .unwrap_or(&ThermostatAction::Off);

            let result = selected_action.execute(&mut state);
            total_reward += result.reward;

            // Aprender de la experiencia
            agent.learn(
                &observation,
                &selected_action.to_action(),
                result.reward,
                &result.new_observation,
                result.done,
            ).await?;

            // Terminar si alcanzó confort óptimo
            if state.calculate_comfort() > 95.0 {
                break;
            }
        }

        if (episode + 1) % 10 == 0 {
            println!("  Episode {}/{}: Reward = {:.2}", episode + 1, episodes, total_reward);
        }
    }

    println!("\n✓ Entrenamiento completado\n");

    // Mostrar política aprendida
    agent.print_policy().await;

    Ok(())
}
```

**Explicación del proceso:**

1. **Inicialización**: Estado aleatorio para explorar diversidad
2. **Observación**: El agente percibe el estado actual
3. **Decisión**: Selecciona acción (explora nueva o usa aprendida)
4. **Ejecución**: Acción modifica el estado
5. **Recompensa**: Señal de qué tan buena fue la acción
6. **Aprendizaje**: Actualiza valores Q(s,a) con la recompensa
7. **Iteración**: Repite hasta alcanzar objetivo o timeout

---

## Paso 6: Detección de anomalías

El modelo predictivo detecta comportamientos anormales:

```rust
// src/anomaly.rs
use hope_agents::HopeAgent;
use crate::thermostat::ThermostatState;

pub async fn detect_anomalies(
    agent: &HopeAgent,
    state: &ThermostatState,
) -> anyhow::Result<bool> {
    let observation = state.to_observation();

    // Predecir siguiente estado esperado
    let prediction = agent.predict_next_state(&observation).await?;

    // Detectar si el estado actual es anómalo
    let is_anomaly = agent.is_anomaly(&observation).await?;

    if is_anomaly {
        println!("⚠️  ANOMALÍA DETECTADA:");
        println!("   Temp: {:.1}°C (objetivo: {:.1}°C)",
            state.current_temp, state.target_temp);
        println!("   Humedad: {:.1}%", state.humidity);
        println!("   Confort: {:.1}%", state.calculate_comfort());

        // Obtener explicación
        if let Some(reason) = agent.explain_anomaly(&observation).await? {
            println!("   Razón: {}", reason);
        }
    }

    Ok(is_anomaly)
}

/// Monitorear continuamente y alertar anomalías
pub async fn monitor_anomalies(
    agent: &HopeAgent,
    states: Vec<ThermostatState>,
) -> anyhow::Result<()> {
    println!("🔍 Monitoreando anomalías...\n");

    let mut anomaly_count = 0;

    for (i, state) in states.iter().enumerate() {
        let is_anomaly = detect_anomalies(agent, state).await?;

        if is_anomaly {
            anomaly_count += 1;
        }

        if (i + 1) % 20 == 0 {
            println!("  Procesadas {} lecturas, {} anomalías detectadas",
                i + 1, anomaly_count);
        }
    }

    println!("\n✓ Monitoreo completado");
    println!("  Total anomalías: {} / {} ({:.1}%)",
        anomaly_count,
        states.len(),
        (anomaly_count as f64 / states.len() as f64) * 100.0
    );

    Ok(())
}
```

**Casos de anomalías detectadas:**

- Temperatura sube/baja muy rápido
- Humedad fuera de rango esperado
- Estado no coincide con predicción del modelo
- Patrón de uso inusual (ej: calefacción en verano)

---

## Paso 7: Toma de decisiones autónoma

El agente ejecuta decisiones automáticamente:

```rust
// src/autonomous.rs
use hope_agents::HopeAgent;
use crate::thermostat::{ThermostatState, ThermostatAction};
use tokio::time::{interval, Duration};

pub async fn run_autonomous_mode(
    agent: &mut HopeAgent,
    initial_state: ThermostatState,
    duration_secs: u64,
) -> anyhow::Result<()> {
    println!("🤖 Modo autónomo activado por {} segundos\n", duration_secs);

    let mut state = initial_state;
    let mut tick_interval = interval(Duration::from_secs(1));
    let end_time = tokio::time::Instant::now() + Duration::from_secs(duration_secs);

    let actions = vec![
        ThermostatAction::HeatHigh,
        ThermostatAction::HeatLow,
        ThermostatAction::Cool,
        ThermostatAction::Fan,
        ThermostatAction::Off,
    ];

    while tokio::time::Instant::now() < end_time {
        tick_interval.tick().await;

        // Observar
        let observation = state.to_observation();

        // Decidir (100% explotación, 0% exploración)
        agent.set_exploration_rate(0.0);
        let action_name = agent.decide_action(&observation).await?;

        // Ejecutar
        let selected_action = actions
            .iter()
            .find(|a| a.action_name() == action_name)
            .unwrap_or(&ThermostatAction::Off);

        let result = selected_action.execute(&mut state);

        // Log
        println!("│ Acción: {:?}", selected_action);
        println!("│ Temp: {:.1}°C → {:.1}°C (objetivo: {:.1}°C)",
            state.current_temp - result.new_observation.features[0],
            state.current_temp,
            state.target_temp
        );
        println!("│ Confort: {:.1}%", state.calculate_comfort());
        println!("│ Recompensa: {:.2}", result.reward);
        println!("└─────────────────────────────────────");

        // Detectar anomalías en tiempo real
        if agent.is_anomaly(&observation).await? {
            println!("⚠️  Comportamiento anómalo detectado");
        }
    }

    println!("\n✓ Modo autónomo finalizado");
    println!("  Temperatura final: {:.1}°C", state.current_temp);
    println!("  Confort final: {:.1}%", state.calculate_comfort());

    Ok(())
}
```

**Explicación:**

- **Exploration rate = 0**: Solo usa lo aprendido, no explora
- **Loop continuo**: Toma decisiones cada segundo
- **Autonomía completa**: No requiere intervención humana
- **Monitoreo**: Detecta anomalías en tiempo real

---

## Paso 8: Programa completo

Integra todos los componentes:

```rust
// src/main.rs
mod thermostat;
mod goals;
mod training;
mod anomaly;
mod autonomous;

use hope_agents::{HopeAgent, AgentConfig};
use thermostat::ThermostatState;
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    // 1. Crear agente
    let config = AgentConfig::ai_mode();
    let mut agent = HopeAgent::new(config)?;
    println!("✓ HOPE Agent creado\n");

    // 2. Entrenar
    training::train_agent(&mut agent, 100).await?;

    // 3. Probar detección de anomalías
    let test_states = vec![
        ThermostatState {
            current_temp: 21.0,
            target_temp: 21.0,
            humidity: 50.0,
            occupancy: true,
            time_of_day: 14,
        },
        ThermostatState {
            current_temp: 35.0, // ANOMALÍA: muy caliente
            target_temp: 21.0,
            humidity: 90.0,     // ANOMALÍA: muy húmedo
            occupancy: false,
            time_of_day: 3,
        },
    ];

    anomaly::monitor_anomalies(&agent, test_states).await?;

    // 4. Ejecutar modo autónomo
    let initial_state = ThermostatState {
        current_temp: 18.0, // Frío
        target_temp: 21.0,
        humidity: 55.0,
        occupancy: true,
        time_of_day: 10,
    };

    autonomous::run_autonomous_mode(&mut agent, initial_state, 60).await?;

    // 5. Guardar agente entrenado
    agent.save_to_file("thermostat_agent.json").await?;
    println!("\n✓ Agente guardado en thermostat_agent.json");

    Ok(())
}
```

---

## Resultado esperado

```
✓ HOPE Agent creado

🎓 Iniciando entrenamiento (100 episodios)...

  Episode 10/100: Reward = 23.45
  Episode 20/100: Reward = 31.20
  Episode 30/100: Reward = 38.67
  ...
  Episode 100/100: Reward = 45.89

✓ Entrenamiento completado

📊 Política aprendida:
  Estado: temp_-3_hum_5_occ_1_hour_10 → Acción: heat_high (Q=8.5)
  Estado: temp_0_hum_5_occ_1_hour_10 → Acción: off (Q=9.2)
  Estado: temp_+2_hum_5_occ_1_hour_10 → Acción: cool (Q=7.8)

🔍 Monitoreando anomalías...

⚠️  ANOMALÍA DETECTADA:
   Temp: 35.0°C (objetivo: 21.0°C)
   Humedad: 90.0%
   Confort: 15.2%
   Razón: Temperatura fuera de rango esperado (+3 σ)

✓ Monitoreo completado
  Total anomalías: 1 / 2 (50.0%)

🤖 Modo autónomo activado por 60 segundos

│ Acción: HeatLow
│ Temp: 18.0°C → 18.5°C (objetivo: 21.0°C)
│ Confort: 72.5%
│ Recompensa: 2.30
└─────────────────────────────────────
│ Acción: HeatLow
│ Temp: 18.5°C → 19.0°C (objetivo: 21.0°C)
│ Confort: 80.0%
│ Recompensa: 0.75
└─────────────────────────────────────
...

✓ Modo autónomo finalizado
  Temperatura final: 21.2°C
  Confort final: 98.5%

✓ Agente guardado en thermostat_agent.json
```

---

## Troubleshooting común

### Agente no aprende

**Problema:** Recompensas siempre bajas, no mejora.

**Solución:**
```rust
config.learning_rate = 0.3;      // Aumentar (más agresivo)
config.exploration_rate = 0.3;   // Más exploración
// Entrenar más episodios
train_agent(&mut agent, 500).await?;
```

### Memoria insuficiente

**Problema:** Error "Out of memory".

**Solución:**
```rust
config.max_memory_bytes = 512 * 1024;  // Reducir a 512 KB
config.max_rules = 100;                // Menos reglas
```

### Anomalías no se detectan

**Problema:** No detecta estados anormales.

**Solución:**
```rust
// Entrenar más para mejorar modelo predictivo
train_agent(&mut agent, 200).await?;

// Ajustar threshold de anomalía
agent.set_anomaly_threshold(2.5); // Más sensible (default: 3.0)
```

---

## Próximos pasos

1. **[Integrar con IoT](./iot-sensor-network.md)**: Controlar sensores reales
2. **[Consultas semánticas](./semantic-queries.md)**: Consultar decisiones del agente
3. **[Persistencia](./getting-started.md)**: Guardar experiencias en el DAG
4. **Producción**: Deploy en edge devices con AIngle minimal

---

## Conceptos clave aprendidos

- **HOPE Agent**: Agente de aprendizaje jerárquico
- **Q-Learning**: Aprende valores de estado-acción
- **Exploration vs Exploitation**: Balance entre explorar y usar lo aprendido
- **Hierarchical Goals**: Descomponer objetivos complejos
- **Anomaly Detection**: Detectar comportamientos inusuales
- **Autonomous Decision-making**: Toma decisiones sin intervención humana

---

## Referencias

- [HOPE Agents Implementation](../../crates/hope_agents/IMPLEMENTATION_SUMMARY.md)
- [Reinforcement Learning: An Introduction](http://incompleteideas.net/book/the-book-2nd.html)
- [Q-Learning Tutorial](https://en.wikipedia.org/wiki/Q-learning)
- [Hierarchical Reinforcement Learning](https://people.cs.umass.edu/~mahadeva/papers/hrl.pdf)
