# Focus Guard Storyboard (YouTube-ready)
This storyboard is visual documentation for assembling a complete walkthrough video without avatar tooling.
## Target output
- Duration: 2:45-3:00
- Format: 16:9, 1920x1080, 30fps
- Audio: clear mono or stereo voiceover, normalized to platform default
- Export: H.264 MP4 (YouTube-friendly)
## Asset checklist
- Title card: `Focus Guard` + subtitle `Deterministic resilience for retry overload`
- Repo overview shot (`README.md`, project root)
- Code shot (`src/lib.rs` request-header decision path + `send_response`)
- Terminal clip: start module with explicit config
- Terminal clip: happy path request (`x-envoy-attempt-count: 1`)
- Terminal clip: overload request (`x-envoy-attempt-count: 3`)
- Terminal clip: `cargo test`
- End card with impact bullets and final line
## Scene-by-scene plan
### Scene 1 — Opening hook (0:00-0:20)
Visual:
- Title card over subtle terminal/log background
Voiceover:
- “During an incident, repeated retries don't just slow down your system. They fragment your attention. Focus Guard is an Envoy extension built to fix both.”
On-screen text:
- `Focus Guard`
- `Deterministic resilience for retry overload`
### Scene 2 — Problem + claim (0:20-0:40)
Visual:
- Simple slide with one problem statement
Voiceover:
- “Focus Guard enforces retry guardrails at the proxy layer — before noise reaches the engineer.”
On-screen text:
- `Problem: retry storms create noise and fragmented attention.`
### Scene 3 — Core logic walkthrough (0:40-1:05)
Visual:
- `src/lib.rs` snippets: retry-attempt parse, threshold comparison, local response path, metrics/metadata writes
Voiceover:
- “At request-header time, Focus Guard reads x-envoy-attempt-count, compares it to a configured threshold, and makes a deterministic decision: pass or throttle.”
- “If the threshold is reached, it sends a local overload response immediately.”
- “Every decision is surfaced through headers, metadata, and metrics for fast debugging and operational trust.”
### Scene 4 — Start Focus Guard locally (1:05-1:25)
Visual:
- Terminal recording running module with explicit config
Voiceover:
- “Now I’m running Focus Guard locally with explicit settings: retry threshold 3, overload status 429, a custom overload message, and AI mode disabled for deterministic baseline behavior.”
- “AI mode is disabled here to establish the deterministic baseline first — the same pattern you'd use in production before enabling enrichment.”
Command:
```bash
boe run --local . --config '{
  "retry_threshold": 3,
  "overload_status_code": 429,
  "overload_body": "Focus Guard: retry overload detected.",
  "enable_tars": false
}'
```
### Scene 5 — Happy path demo (1:25-1:50)
Visual:
- Terminal request and response
- Callout overlays for headers
Voiceover:
- “First, the happy path.”
- “With attempt count 1, the request passes upstream normally.”
- “Focus Guard still annotates the outcome so behavior is always observable.”
Command:
```bash
curl -i -H "x-envoy-attempt-count: 1" http://localhost:10000/status/200
```
Callouts:
- `x-focus-guard: pass`
- `x-focus-guard-retry-attempt: 1`
### Scene 6 — Retry overload demo (1:50-2:20)
Visual:
- Terminal request and response with status + header callouts
Voiceover:
- “Now I trigger the retry overload path by sending attempt count 3, which matches the configured threshold.”
- “Focus Guard responds locally with status 429 and the configured overload body, instead of allowing retries to keep escalating.”
- “This converts ambiguous retry churn into a clear, debuggable state.”
- “The engineer sees exactly one signal, not a cascade.”
Command:
```bash
curl -i -H "x-envoy-attempt-count: 3" http://localhost:10000/status/200
```
Callouts:
- `429`
- configured overload body
- `x-focus-guard: throttled`
- `x-focus-guard-retry-attempt: 3`
- `x-focus-guard-tars: disabled` (or `active` if `enable_tars=true`)
### Scene 7 — Validation step (2:20-2:35)
Visual:
- Terminal test run and pass summary
Voiceover:
- “To validate reliability, I run the test suite.”
- “These tests cover config parsing, fallback behavior, and deterministic pass-versus-throttle logic.”
Command:
```bash
cargo test
```
### Scene 8 — Closing pitch (2:35-2:55)
Visual:
- Clean slide with two-line closing claim
Voiceover:
- “Focus Guard demonstrates that neurodivergent-centered design can be implemented as concrete reliability controls.”
- “It turns noisy retry behavior into explicit, predictable states that teams can trust under pressure.”
- “It was designed to be extended — but safe by default.”
### Scene 9 — Impact summary + end card (2:55-3:00)
Visual:
- End card with impact bullets
Voiceover:
- “The impact is practical and immediate: lower cognitive overload during incidents, clearer operational signals, and safe extensibility for future enhancements.”
Impact bullets:
- `Reduces cognitive overload in retry storms`
- `Makes pass-versus-throttle decisions explicit and debuggable`
- `Preserves deterministic safety while enabling future async AI enrichment`
Final line:
- `Focus Guard: deterministic resilience for calmer, clearer incident response.`
## Assembly workflow (fast path)
1. Capture each scene as short clips (10-30 seconds) using your preferred screen recorder.
2. Import clips into a basic editor (iMovie, CapCut, DaVinci Resolve, Final Cut, Premiere).
3. Follow scene order exactly; add title cards and callout overlays where specified.
4. Record voiceover directly from the script lines above.
5. Export `H.264 MP4`, `1080p`, `30fps`.
6. Upload to YouTube with:
   - title: `Focus Guard Demo — Deterministic Retry Guardrails for Envoy`
   - description: project purpose + command snippets
   - chapters aligned to Scene 1 through Scene 9
