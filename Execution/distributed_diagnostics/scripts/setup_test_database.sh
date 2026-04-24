#!/usr/bin/env bash
set -euo pipefail

DB_CONTAINER="${DB_CONTAINER:-postgres}"
DB_USER="${DB_USER:-postgres}"
DB_PASSWORD="${DB_PASSWORD:-postgres}"
DB_NAME="${DB_NAME:-distributed_diagnostics_test}"
ROOT_DB="${ROOT_DB:-postgres}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"

echo "Ensuring test database '${DB_NAME}' exists in container '${DB_CONTAINER}'..."

if ! docker exec "${DB_CONTAINER}" psql -U "${DB_USER}" -d "${ROOT_DB}" -tAc "SELECT 1 FROM pg_database WHERE datname = '${DB_NAME}'" | grep -q 1; then
  docker exec "${DB_CONTAINER}" psql -U "${DB_USER}" -d "${ROOT_DB}" -c "CREATE DATABASE ${DB_NAME};"
fi

echo "Applying diagnostics schema initialization..."
docker exec -i "${DB_CONTAINER}" psql -U "${DB_USER}" -d "${DB_NAME}" -v ON_ERROR_STOP=1 -f - \
  < "${REPO_ROOT}/Execution/docker/postgres/init/101_diagnostics_incident_cards.sql"
docker exec -i "${DB_CONTAINER}" psql -U "${DB_USER}" -d "${DB_NAME}" -v ON_ERROR_STOP=1 -f - \
  < "${REPO_ROOT}/Execution/docker/postgres/init/102_diagnostics_run_state.sql"

echo "Test database is ready."
echo "Use:"
echo "  export TEST_DATABASE_URL=postgres://${DB_USER}:${DB_PASSWORD}@localhost:5432/${DB_NAME}"
echo "  cp Execution/distributed_diagnostics/.env.test.example Execution/distributed_diagnostics/.env.test"
echo "  cargo test --manifest-path Execution/distributed_diagnostics/Cargo.toml --features postgres-integration --test run_state_store_postgres"
