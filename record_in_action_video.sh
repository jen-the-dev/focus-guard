#!/usr/bin/env bash
set -euo pipefail

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SUBMISSION_DIR="$PROJECT_DIR/submission-package"
MEDIA_DIR="$SUBMISSION_DIR/media"
EVIDENCE_DIR="$SUBMISSION_DIR/evidence"
VIDEO_FILE="$MEDIA_DIR/focus-guard-in-action.mov"
TRANSCRIPT_FILE="$EVIDENCE_DIR/in-action-transcript.txt"
DEMO_RUNNER="$PROJECT_DIR/demo_in_action.sh"
DISPLAY_INDEX="${DEMO_DISPLAY_INDEX:-1}"
MAX_SECONDS="${DEMO_MAX_SECONDS:-150}"

mkdir -p "$MEDIA_DIR" "$EVIDENCE_DIR"
rm -f "$VIDEO_FILE" "$TRANSCRIPT_FILE"

echo "Starting in-action screen recording (display=$DISPLAY_INDEX, max=${MAX_SECONDS}s)..."
screencapture -v -D"$DISPLAY_INDEX" -V"$MAX_SECONDS" "$VIDEO_FILE" &
RECORD_PID=$!

cleanup() {
  if kill -0 "$RECORD_PID" 2>/dev/null; then
    kill -INT "$RECORD_PID" 2>/dev/null || true
    wait "$RECORD_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

sleep 2
script -q "$TRANSCRIPT_FILE" /bin/bash "$DEMO_RUNNER"

cleanup
trap - EXIT

if [[ ! -s "$VIDEO_FILE" ]]; then
  echo "❌ In-action demo video was not created: $VIDEO_FILE" >&2
  exit 1
fi

if [[ ! -s "$TRANSCRIPT_FILE" ]]; then
  echo "❌ In-action transcript was not created: $TRANSCRIPT_FILE" >&2
  exit 1
fi

echo "✅ In-action demo video created: $VIDEO_FILE"
echo "✅ In-action transcript created: $TRANSCRIPT_FILE"
