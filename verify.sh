#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LISTEN_PORT="${LISTEN_PORT:-10000}"
ADMIN_PORT="${ADMIN_PORT:-9901}"
BASE_URL="${BASE_URL:-http://localhost:${LISTEN_PORT}}"
STARTUP_TIMEOUT_SECONDS="${STARTUP_TIMEOUT_SECONDS:-60}"
UPSTREAM_HOST="${UPSTREAM_HOST:-localhost}"
UPSTREAM_PORT="${UPSTREAM_PORT:-18080}"
BOE_GEN_CONFIG_ARGS="${BOE_GEN_CONFIG_ARGS:-}"

CONFIG_JSON='{
  "retry_threshold": 3,
  "overload_status_code": 429,
  "overload_body": "Focus Guard: retry overload detected.",
  "enable_tars": false
}'

TMP_DIR="$(mktemp -d)"
LOG_FILE="$TMP_DIR/envoy.log"
BOE_LOG_FILE="$TMP_DIR/boe-gen-config.log"
UPSTREAM_LOG_FILE="$TMP_DIR/upstream.log"
EXPORT_DIR="$TMP_DIR/export"
ENVOY_CONFIG_FILE="$EXPORT_DIR/envoy.yaml"
ENVOY_PATH_RESOLVED=""
UPSTREAM_PID=""
ENVOY_PID=""
FAILURES=0

cleanup() {
  local exit_code=$?

  if [[ -n "$ENVOY_PID" ]] && kill -0 "$ENVOY_PID" 2>/dev/null; then
    kill "$ENVOY_PID" 2>/dev/null || true
    wait "$ENVOY_PID" 2>/dev/null || true
  fi

  if [[ -n "$UPSTREAM_PID" ]] && kill -0 "$UPSTREAM_PID" 2>/dev/null; then
    kill "$UPSTREAM_PID" 2>/dev/null || true
    wait "$UPSTREAM_PID" 2>/dev/null || true
  fi

  if [[ $exit_code -ne 0 ]]; then
    echo "Verification artifacts kept at: $TMP_DIR"
    echo "envoy log: $LOG_FILE"
    echo "boe gen-config log: $BOE_LOG_FILE"
    echo "upstream log: $UPSTREAM_LOG_FILE"
  else
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "❌ Required command not found: $cmd" >&2
    exit 1
  fi
}

add_path_if_dir() {
  local dir_path="$1"
  if [[ -d "$dir_path" ]] && [[ ":$PATH:" != *":$dir_path:"* ]]; then
    PATH="$dir_path:$PATH"
  fi
}

ensure_toolchain_path() {
  add_path_if_dir "$HOME/.cargo/bin"
  export PATH
}

ensure_dynamic_module_linker_flags() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    return
  fi

  local dynamic_lookup_flag="-C link-arg=-Wl,-undefined,dynamic_lookup"
  if [[ " ${RUSTFLAGS:-} " != *" ${dynamic_lookup_flag} "* ]]; then
    export RUSTFLAGS="${RUSTFLAGS:+${RUSTFLAGS} }${dynamic_lookup_flag}"
  fi
}

assert_equals() {
  local label="$1"
  local expected="$2"
  local actual="$3"

  if [[ "$expected" == "$actual" ]]; then
    echo "✅ $label: $actual"
  else
    echo "❌ $label: expected '$expected' but got '$actual'"
    FAILURES=$((FAILURES + 1))
  fi
}

assert_contains() {
  local label="$1"
  local expected_substring="$2"
  local file_path="$3"

  if grep -Fq "$expected_substring" "$file_path"; then
    echo "✅ $label contains expected text"
  else
    echo "❌ $label missing expected text: $expected_substring"
    FAILURES=$((FAILURES + 1))
  fi
}

get_status_code() {
  local headers_file="$1"
  awk '/^HTTP\// { code=$2 } END { print code }' "$headers_file"
}

get_header_value() {
  local headers_file="$1"
  local header_name="$2"
  awk -F': ' -v target="$header_name" '
    BEGIN { IGNORECASE=1 }
    tolower($1) == tolower(target) {
      value=$2
      sub(/\r$/, "", value)
      print value
      exit
    }
  ' "$headers_file"
}

resolve_envoy_path() {
  local data_home="${BOE_DATA_HOME:-$HOME/.local/share/boe}"

  if [[ -n "${ENVOY_PATH:-}" ]]; then
    if [[ -x "$ENVOY_PATH" ]]; then
      echo "$ENVOY_PATH"
      return 0
    fi
    echo "❌ ENVOY_PATH is set but not executable: $ENVOY_PATH" >&2
    return 1
  fi

  if [[ -n "${ENVOY_VERSION:-}" ]]; then
    local version_path="$data_home/envoy-versions/$ENVOY_VERSION/bin/envoy"
    if [[ -x "$version_path" ]]; then
      echo "$version_path"
      return 0
    fi
  fi

  local candidate=""
  local path_candidate
  for path_candidate in "$data_home"/envoy-versions/*/bin/envoy; do
    if [[ -x "$path_candidate" ]]; then
      candidate="$path_candidate"
    fi
  done

  if [[ -n "$candidate" ]]; then
    echo "$candidate"
    return 0
  fi

  return 1
}

prime_envoy_download() {
  local prime_args=(--local "$PROJECT_DIR" --config "$CONFIG_JSON" --run-id "focus-guard-verify-prime")
  if [[ -n "${ENVOY_VERSION:-}" ]]; then
    prime_args+=(--envoy-version "$ENVOY_VERSION")
  fi

  echo "Priming Envoy download via boe run (one-time setup)..."
  boe run "${prime_args[@]}" >/dev/null 2>&1 || true
}

start_local_upstream() {
  echo "Starting local upstream at ${UPSTREAM_HOST}:${UPSTREAM_PORT}..."
  (
    python3 - "$UPSTREAM_HOST" "$UPSTREAM_PORT" <<'PY'
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

host = sys.argv[1]
port = int(sys.argv[2])

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        body = b"ok\n"
        self.send_response(200)
        self.send_header("Content-Type", "text/plain")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, _fmt, *_args):
        return

HTTPServer((host, port), Handler).serve_forever()
PY
  ) >"$UPSTREAM_LOG_FILE" 2>&1 &
  UPSTREAM_PID=$!
  echo "upstream PID: $UPSTREAM_PID"
}

wait_for_upstream() {
  local deadline=$((SECONDS + STARTUP_TIMEOUT_SECONDS))
  local upstream_url="http://${UPSTREAM_HOST}:${UPSTREAM_PORT}/status/200"

  echo "Waiting for local upstream readiness at $upstream_url ..."
  while (( SECONDS < deadline )); do
    if curl -sS -o /dev/null "$upstream_url"; then
      echo "Local upstream is ready."
      return 0
    fi

    if [[ -n "$UPSTREAM_PID" ]] && ! kill -0 "$UPSTREAM_PID" 2>/dev/null; then
      echo "❌ Local upstream process exited early. Check log: $UPSTREAM_LOG_FILE"
      return 1
    fi

    sleep 1
  done

  echo "❌ Timed out waiting for local upstream readiness after ${STARTUP_TIMEOUT_SECONDS}s."
  return 1
}

generate_exported_config() {
  mkdir -p "$EXPORT_DIR"
  local boe_args=(
    gen-config
    --local "$PROJECT_DIR"
    --config "$CONFIG_JSON"
    --listen-port "$LISTEN_PORT"
    --admin-port "$ADMIN_PORT"
    --cluster-insecure "${UPSTREAM_HOST}:${UPSTREAM_PORT}"
    --test-upstream-cluster "${UPSTREAM_HOST}:${UPSTREAM_PORT}"
    --output "$EXPORT_DIR"
  )

  local extra_args=()
  if [[ -n "$BOE_GEN_CONFIG_ARGS" ]]; then
    read -r -a extra_args <<<"$BOE_GEN_CONFIG_ARGS"
    boe_args+=("${extra_args[@]}")
  fi

  echo "Generating Envoy config with boe..."
  boe "${boe_args[@]}" >"$BOE_LOG_FILE" 2>&1
}

patch_envoy_config_if_needed() {
  if grep -Eq '^[[:space:]]*metrics_namespace:' "$ENVOY_CONFIG_FILE"; then
    echo "Patching Envoy config: removing unsupported dynamic_module_config.metrics_namespace ..."
    awk '!/^[[:space:]]*metrics_namespace:[[:space:]]*/ { print }' "$ENVOY_CONFIG_FILE" >"$ENVOY_CONFIG_FILE.patched"
    mv "$ENVOY_CONFIG_FILE.patched" "$ENVOY_CONFIG_FILE"
  fi
}

start_envoy() {
  ENVOY_PATH_RESOLVED="$(resolve_envoy_path || true)"
  if [[ -z "$ENVOY_PATH_RESOLVED" ]]; then
    prime_envoy_download
    ENVOY_PATH_RESOLVED="$(resolve_envoy_path || true)"
  fi

  if [[ -z "$ENVOY_PATH_RESOLVED" ]]; then
    echo "❌ Could not resolve an Envoy binary. Set ENVOY_PATH or ENVOY_VERSION."
    return 1
  fi

  echo "Starting Envoy with patched bootstrap..."
  ENVOY_DYNAMIC_MODULES_SEARCH_PATH="$EXPORT_DIR" \
    GODEBUG=cgocheck=0 \
    "$ENVOY_PATH_RESOLVED" -c "$ENVOY_CONFIG_FILE" --log-level error >"$LOG_FILE" 2>&1 &
  ENVOY_PID=$!
  echo "envoy PID: $ENVOY_PID"
}

wait_for_service() {
  local deadline=$((SECONDS + STARTUP_TIMEOUT_SECONDS))

  echo "Waiting for service readiness at $BASE_URL ..."
  while (( SECONDS < deadline )); do
    if curl -sS -o /dev/null "$BASE_URL/status/200"; then
      echo "Service is ready."
      return 0
    fi

    if [[ -n "$ENVOY_PID" ]] && ! kill -0 "$ENVOY_PID" 2>/dev/null; then
      echo "❌ Envoy process exited early. Check log: $LOG_FILE"
      return 1
    fi

    sleep 1
  done

  echo "❌ Timed out waiting for service readiness after ${STARTUP_TIMEOUT_SECONDS}s."
  return 1
}

run_case() {
  local name="$1"
  local attempt_value="$2"
  local expected_status="$3"
  local expected_guard="$4"
  local expected_retry_attempt="$5"
  local expected_tars="$6"
  local expected_body_substring="$7"

  local headers_file="$TMP_DIR/${name}.headers"
  local body_file="$TMP_DIR/${name}.body"

  echo
  echo "Running case: $name"

  local curl_args=(-sS -D "$headers_file" -o "$body_file")
  if [[ -n "$attempt_value" ]]; then
    curl_args+=(-H "x-envoy-attempt-count: $attempt_value")
  fi

  curl "${curl_args[@]}" "$BASE_URL/status/200"

  local actual_status
  local actual_guard
  local actual_retry_attempt
  local actual_tars

  actual_status="$(get_status_code "$headers_file")"
  actual_guard="$(get_header_value "$headers_file" "x-focus-guard")"
  actual_retry_attempt="$(get_header_value "$headers_file" "x-focus-guard-retry-attempt")"
  actual_tars="$(get_header_value "$headers_file" "x-focus-guard-tars")"

  assert_equals "$name status" "$expected_status" "$actual_status"
  assert_equals "$name x-focus-guard" "$expected_guard" "$actual_guard"
  assert_equals "$name x-focus-guard-retry-attempt" "$expected_retry_attempt" "$actual_retry_attempt"
  assert_equals "$name x-focus-guard-tars" "$expected_tars" "$actual_tars"

  if [[ -n "$expected_body_substring" ]]; then
    assert_contains "$name body" "$expected_body_substring" "$body_file"
  fi
}

main() {
  ensure_toolchain_path
  ensure_dynamic_module_linker_flags
  require_command boe
  require_command cargo
  require_command curl
  require_command python3

  start_local_upstream
  wait_for_upstream
  generate_exported_config
  patch_envoy_config_if_needed
  start_envoy
  wait_for_service

  run_case "happy_path" "1" "200" "pass" "1" "disabled" ""
  run_case "retry_overload" "3" "429" "throttled" "3" "disabled" "Focus Guard: retry overload detected."
  run_case "missing_header" "" "200" "pass" "1" "disabled" ""
  run_case "invalid_header" "abc" "200" "pass" "1" "disabled" ""

  echo
  if (( FAILURES > 0 )); then
    echo "❌ Runtime verification failed with $FAILURES issue(s)."
    echo "Check envoy log: $LOG_FILE"
    exit 1
  fi

  echo "✅ Runtime verification passed."
}

main "$@"