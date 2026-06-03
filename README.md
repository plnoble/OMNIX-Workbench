# OMNIX Development Tools

This project is developed under the AI Development Memory & Engineering Protocol.

## Engineering Principles
- **Modular & Decoupled Architecture**: High cohesion, low coupling, Dependency Injection, Program to Interfaces, Clean Layering.
- **SOLID, DRY, KISS, YAGNI**: No premature optimization or unnecessary complexity.
- **Strict API Design**: Versioned APIs (`/v1/...`), idempotent write operations, unified schemas.
- **Security & Privacy by Design**: SQL parameters/ORM, HTML escaping, JWT & HTTPS, minified PII collection, auto-scrubbed logs.
- **Type Safety & Quality**: TS strict mode, no `any`, semantic naming, <= 30 line functions.
- **Unified Error Handling**: Fail Fast, exponential backoff with jitter for retries, friendly public errors.
- **Database Migrations**: Versioned migrations, standard audit columns (`created_at`, `updated_at`).
- **i18n & UTC Time**: Transmit in UTC, display in local time zone, externalized resources.
- **Structured Logs & Observability**: JSON structured logging, trace propagation, metrics & health endpoints.

## Development Memory Structure
- `/logs/tasks/` - Task status logs
- `/logs/decisions/` - Key technical decisions
- `/logs/bugs/` - Debugging and bug tracing
- `/logs/reviews/` - Peer / Self review reports
- `/logs/reflections/` - Post-task retrospectives
- `/logs/timeline/` - Milestone timelines
- `/memory/working_memory/` - Active execution context
- `/memory/episodic_memory/` - Ephemeral/event patterns
- `/memory/semantic_memory/` - Project knowledge base
- `/memory/skill_memory/` - Distilled automation actions
