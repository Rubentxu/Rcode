# RCode

Plataforma de agente de codificación IA construida en Rust, diseñada para ejecución de herramientas, persistencia de sesiones e interoperabilidad entre múltiples proveedores.

## Descripción General

RCode es un agente de codificación IA construido completamente en Rust, orientado a alto rendimiento y ejecución nativa en múltiples interfaces: API HTTP, TUI interactiva, cliente web y escritorio nativo mediante Tauri.

El agente destaca en tareas de código combinando un potente sistema de herramientas con respuestas en streaming y soporte multi-proveedor. Puede leer, escribir y editar archivos, ejecutar comandos shell, buscar en bases de código, delegar a sub-agentes e integrar con servicios externos mediante MCP.

## Características Principales

- **Rendimiento Nativo** — Construido en Rust con Tokio para operaciones asíncronas, ofreciendo ejecución eficiente de herramientas y streaming de baja latencia
- **Multi-Proveedor** — Compatible con Anthropic, OpenAI, Google, MiniMax, ZAI, OpenRouter y otros proveedores compatibles con OpenAI
- **Sistema de Herramientas** — Más de 17 herramientas integradas: bash, read, write, edit, glob, grep, task, skill, webfetch, websearch, MCP y más
- **Streaming SSE** — Eventos en tiempo real para texto, razonamiento y ejecución de herramientas
- **Multi-Cliente** — Servidor HTTP, TUI interactiva, interfaz web y aplicación de escritorio Tauri
- **Persistencia de Sesiones** — Almacenamiento basado en SQLite con historial de mensajes
- **Skills y Comandos** — Carga de instrucciones especializadas y comandos slash en tiempo de ejecución
- **Integración MCP** — Conexión a servidores Model Context Protocol para capacidades extendidas
- **Privacy Gateway** — Ganchos de sanitización de datos y monitorización de seguridad (vía `crates/privacy`)
- **CogniCode** — Inyección de inteligencia de código para contexto mejorado (vía `crates/cognicode`)

## Arquitectura

```
┌──────────────────────────────────────────────────────────────┐
│                    Capa de Cliente                           │
│         (CLI / TUI / Web / Escritorio Tauri)                │
└────────────────────────────┬─────────────────────────────────┘
                             │ HTTP + SSE
┌────────────────────────────▼─────────────────────────────────┐
│                    rcode-server (Axum)                       │
│  submit_prompt() ──► AgentExecutor Loop                       │
│                             │                                 │
│         ┌───────────────────┼───────────────────┐              │
│         ▼                   ▼                   ▼              │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ LlmProvider │  │ToolRegistry  │  │  EventBus    │       │
│  │ (streaming) │  │  Service     │  │  (SSE pub)  │       │
│  └─────────────┘  └──────────────┘  └──────────────┘       │
│                             │                                 │
│         ┌───────────────────┼───────────────────┐              │
│         ▼                   ▼                   ▼              │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────┐       │
│  │ Core Tools  │  │  MCP Tools   │  │  Skills/     │       │
│  │ (bash,etc.) │  │              │  │  Commands    │       │
│  └─────────────┘  └──────────────┘  └──────────────┘       │
└──────────────────────────────────────────────────────────────┘
```

## Estructura del Repositorio

```
rcode/
├── crates/                    # Miembros del workspace Rust
│   ├── core/                  # Tipos de dominio, traits, modelos Message/Part
│   ├── agent/                # AgentExecutor, DefaultAgent, gestión de subagentes
│   ├── session/              # Servicio de sesión con compactación
│   ├── tools/                # Implementaciones de herramientas y registro
│   ├── providers/            # Adaptadores de proveedores LLM (OpenAI, Anthropic, etc.)
│   ├── server/               # Servidor HTTP con Axum, API REST, SSE
│   ├── storage/              # Capa de persistencia SQLite
│   ├── event/                # EventBus para streaming SSE
│   ├── cli/                  # Aplicación CLI (comandos run, serve, tui)
│   ├── tui/                  # Interfaz de terminal interactiva (Ratatui)
│   ├── config/               # Carga y gestión de configuración
│   ├── mcp/                  # Cliente MCP y registro de servidores
│   ├── engram/              # Sistema de memoria persistente
│   ├── acp/                  # Protocolo de Comunicación de Agentes
│   ├── privacy/              # Servicio de privacidad
│   ├── cognicode/            # Inyección de inteligencia de código
│   ├── observability/        # Trazas y métricas
│   ├── plugins/              # Cargador y gestor de plugins
│   └── gen-types/           # Generación de tipos para el frontend
├── web/                      # Cliente web (SolidJS + Vite + Tailwind)
│   ├── src/                  # Componentes SolidJS y cliente API
│   ├── e2e/                  # Tests E2E con Playwright
│   └── src-tauri/            # Configuración del escritorio Tauri
└── docs/                     # Documentación de arquitectura y diseño
```

## Primeros Pasos

### Requisitos Previos

- **Rust 1.85+** (edition 2024)
- **SQLite** (para persistencia de sesiones)
- **Clave API** para tu proveedor elegido (Anthropic, OpenAI, etc.)

### Compilar desde el Fuente

```bash
# Clonar el repositorio
git clone <url-de-tu-fork>
cd rust-code

# Compilar todo el workspace
cargo build --workspace

# O compilar solo el CLI
cargo build -p rcode-cli
```

### Inicio Rápido

```bash
# Configurar tu clave API
export ANTHROPIC_API_KEY=sk-ant-...

# Ejecutar con un prompt directo
cargo run -p rcode-cli -- run --message "Explica el modelo de propiedad de Rust"

# Iniciar el servidor de API HTTP
cargo run -p rcode-cli -- serve

# Lanzar la TUI interactiva
cargo run -p rcode-cli -- tui
```

El servidor inicia en `http://127.0.0.1:4096` por defecto.

## Configuración

RCode busca archivos de configuración en este orden:

1. Ruta pasada mediante `--config <ruta>`
2. `./opencode.json` (directorio actual)
3. `~/.config/opencode/opencode.json` (Unix)
4. Configuraciones en directorio `.opencode/` del proyecto
5. Variable `OPENCODE_CONFIG_CONTENT` (JSON inline)
6. `~/.config/rcode/config.json` (overlay de RCode — mayor prioridad para campos específicos de RCode)

### Ejemplo de Configuración

```json
{
  "model": "anthropic/claude-sonnet-4-5",
  "providers": {
    "anthropic": {
      "api_key": "${ANTHROPIC_API_KEY}"
    },
    "openai": {
      "api_key": "${OPENAI_API_KEY}"
    }
  },
  "server": {
    "port": 4096
  }
}
```

### Variables de Entorno

| Variable | Descripción |
|----------|-------------|
| `ANTHROPIC_API_KEY` | Clave API de Anthropic |
| `OPENAI_API_KEY` | Clave API de OpenAI |
| `OPENROUTER_API_KEY` | Clave API de OpenRouter |
| `MINIMAX_API_KEY` | Clave API de MiniMax |
| `OPENCODE_CONFIG_CONTENT` | JSON de configuración inline (mayor prioridad tras `--config`) |

## Comandos CLI

### `run` — Ejecución Directa

```bash
rcode run [OPCIONES]

Opciones:
  -m, --message <MENSAJE>    Mensaje directo de entrada
  -f, --file <ARCHIVO>       Leer prompt desde archivo
      --stdin                Leer desde stdin
      --json                 Salida como JSON
      --silent               Suprimir stdout
      --save-session <BOOL>  Persistir sesión (por defecto: true)
  -s, --model <MODELO>       Modelo a utilizar
```

### `serve` — Modo Servidor HTTP

```bash
rcode serve [OPCIONES]

Opciones:
  -p, --port <PUERTO>     Puerto de escucha (por defecto: 4096)
  -h, --hostname <HOST>    Hostname de enlace (por defecto: 127.0.0.1)
```

### `tui` — Terminal Interactiva

```bash
rcode tui
```

## API HTTP

### Endpoints de Sesión

| Método | Endpoint | Descripción |
|--------|----------|-------------|
| GET | `/health` | Verificación de salud |
| GET | `/session` | Listar todas las sesiones |
| POST | `/session` | Crear nueva sesión |
| GET | `/session/:id` | Obtener sesión por ID |
| DELETE | `/session/:id` | Eliminar sesión |
| GET | `/session/:id/messages` | Obtener mensajes de sesión (paginado) |
| POST | `/session/:id/prompt` | Enviar prompt a la sesión |
| POST | `/session/:id/abort` | Abortar sesión en ejecución |
| GET | `/session/:id/events` | Stream SSE para eventos de sesión |
| GET | `/event` | Stream SSE para todos los eventos |

### Ejemplo de Uso

```bash
# Crear sesión
curl -X POST http://localhost:4096/session \
  -H "Content-Type: application/json" \
  -d '{"project_path": "/ruta/al/proyecto"}'

# Enviar prompt
curl -X POST http://localhost:4096/session/<session_id>/prompt \
  -H "Content-Type: application/json" \
  -d '{"prompt": "¡Hola, mundo!"}'

# Escuchar eventos
curl http://localhost:4096/session/<session_id>/events
```

## Sistema de Herramientas

El registro de herramientas por defecto de RCode incluye aproximadamente 20 herramientas integradas:

| Herramienta | Descripción |
|-------------|-------------|
| `bash` | Ejecutar comandos de shell |
| `read` | Leer archivos del sistema |
| `write` | Escribir contenido en archivos |
| `edit` | Realizar ediciones específicas con oldString/newString |
| `multiedit` | Aplicar múltiples ediciones en una sola operación |
| `glob` | Buscar archivos por patrones |
| `grep` | Buscar contenido en archivos |
| `codesearch` | Buscar en bases de código con resultados estructurados |
| `task` | Delegar trabajo a un sub-agente |
| `delegate` / `delegation_read` | Crear y leer registros de delegación |
| `skill` | Cargar instrucciones de skills especializados |
| `slash_command` | Ejecutar comandos slash descubiertos |
| `plan` / `plan_exit` | Mostrar y modificar planes de ejecución |
| `todowrite` | Gestionar listas de tareas |
| `question` | Hacer preguntas clarificadoras al usuario |
| `webfetch` | Obtener contenido de URLs |
| `websearch` | Buscar en la web |
| `applypatch` | Aplicar parches a archivos |
| `session_navigation` | Navegar y consultar el historial de sesiones |

Integraciones de herramientas opcionales (disponibles cuando están configuradas):

| Integración | Herramientas |
|-------------|--------------|
| **CogniCode** | 21 herramientas de inteligencia de código: `cognicode_build_graph`, `cognicode_call_hierarchy`, `cognicode_trace_path`, `cognicode_entry_points`, `cognicode_leaf_functions`, `cognicode_hot_paths`, `cognicode_export_mermaid`, `cognicode_impact_analysis`, `cognicode_complexity`, `cognicode_architecture_check`, `cognicode_semantic_search`, `cognicode_get_symbols`, `cognicode_get_outline`, `cognicode_find_usages`, `cognicode_go_to_definition`, `cognicode_hover`, `cognicode_find_references`, `cognicode_safe_refactor`, `cognicode_validate_syntax`, y más |
| **MCP** | Herramientas dinámicas de servidores Model Context Protocol |

## Pruebas

```bash
# Ejecutar todas las pruebas del workspace
cargo test --workspace

# Ejecutar pruebas de un crate específico
cargo test -p rcode-cli
cargo test -p rcode-agent

# Ejecutar con salida visible
cargo test --workspace -- --nocapture

# Ejecutar linting con Clippy
cargo clippy --workspace --all-targets -- -D warnings
```

### Pruebas del Frontend

```bash
cd web

# Ejecutar pruebas unitarias con vitest
npm test

# Ejecutar tests E2E con Playwright
npm run e2e:web
```

## Desarrollo

### Archivos Clave

- `AGENTS.md` — Contrato del agente y política de validación para contribuyentes
- `docs/architecture/mvp-architecture.md` — Diagramas de arquitectura del sistema
- `docs/architecture-agent-system.md` — Análisis detallado del bucle del agente
- `docs/analysis/opencode-vs-rcode.md` — Análisis comparativo con OpenCode

### Generación de Código

Generar tipos TypeScript desde tipos Rust para el frontend:

```bash
cd web
npm run types:generate
```

## Licencia

MIT
