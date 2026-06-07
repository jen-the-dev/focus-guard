// Copyright Built On Envoy
// SPDX-License-Identifier: Apache-2.0
// The full text of the Apache license is available in the LICENSE file at
// the root of the repo.

use envoy_proxy_dynamic_modules_rust_sdk::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

const DEFAULT_RETRY_THRESHOLD: u32 = 3;
const DEFAULT_OVERLOAD_STATUS_CODE: u32 = 429;
const DEFAULT_OVERLOAD_BODY: &str = "Focus Guard throttled request due to retry overload.";
const DYNAMIC_METADATA_NAMESPACE: &str = "focus_guard";
const DEFAULT_TARS_CLUSTER: &str = "api.router.tetrate.ai:443";
const DEFAULT_TARS_AUTHORITY: &str = "api.router.tetrate.ai";
const DEFAULT_TARS_PATH: &str = "/v1/chat/completions";
const DEFAULT_TARS_MODEL: &str = "o3-mini";
const DEFAULT_TARS_TIMEOUT_MILLISECONDS: u64 = 250;
const DEFAULT_TARS_FAIL_OPEN: bool = true;

fn default_retry_threshold() -> u32 {
    DEFAULT_RETRY_THRESHOLD
}

fn default_overload_status_code() -> u32 {
    DEFAULT_OVERLOAD_STATUS_CODE
}

fn default_overload_body() -> String {
    DEFAULT_OVERLOAD_BODY.to_string()
}
fn default_tars_cluster() -> String {
    DEFAULT_TARS_CLUSTER.to_string()
}

fn default_tars_authority() -> String {
    DEFAULT_TARS_AUTHORITY.to_string()
}

fn default_tars_path() -> String {
    DEFAULT_TARS_PATH.to_string()
}

fn default_tars_model() -> String {
    DEFAULT_TARS_MODEL.to_string()
}

fn default_tars_timeout_milliseconds() -> u64 {
    DEFAULT_TARS_TIMEOUT_MILLISECONDS
}

fn default_tars_fail_open() -> bool {
    DEFAULT_TARS_FAIL_OPEN
}

// The raw filter config that will be deserialized from the JSON configuration.
#[derive(Serialize, Deserialize, Debug)]
pub struct RawFilterConfig {
    #[serde(default = "default_retry_threshold")]
    retry_threshold: u32,
    #[serde(default = "default_overload_status_code")]
    overload_status_code: u32,
    #[serde(default = "default_overload_body")]
    overload_body: String,
    #[serde(default)]
    enable_tars: bool,
    #[serde(default = "default_tars_cluster")]
    tars_cluster: String,
    #[serde(default = "default_tars_authority")]
    tars_authority: String,
    #[serde(default = "default_tars_path")]
    tars_path: String,
    #[serde(default = "default_tars_model")]
    tars_model: String,
    #[serde(default = "default_tars_timeout_milliseconds")]
    tars_timeout_milliseconds: u64,
    #[serde(default = "default_tars_fail_open")]
    tars_fail_open: bool,
    #[serde(default)]
    tars_api_key: Option<String>,
}

impl Default for RawFilterConfig {
    fn default() -> Self {
        RawFilterConfig {
            retry_threshold: DEFAULT_RETRY_THRESHOLD,
            overload_status_code: DEFAULT_OVERLOAD_STATUS_CODE,
            overload_body: DEFAULT_OVERLOAD_BODY.to_string(),
            enable_tars: false,
            tars_cluster: default_tars_cluster(),
            tars_authority: default_tars_authority(),
            tars_path: default_tars_path(),
            tars_model: default_tars_model(),
            tars_timeout_milliseconds: default_tars_timeout_milliseconds(),
            tars_fail_open: default_tars_fail_open(),
            tars_api_key: None,
        }
    }
}

#[derive(Debug)]
pub struct FilterConfigImpl {
    retry_threshold: u32,
    overload_status_code: u32,
    overload_body: String,
    enable_tars: bool,
    tars_cluster: String,
    tars_authority: String,
    tars_path: String,
    tars_model: String,
    tars_timeout_milliseconds: u64,
    tars_fail_open: bool,
    tars_api_key: Option<String>,
    requests_total_counter: Option<EnvoyCounterId>,
    throttled_total_counter: Option<EnvoyCounterId>,
    retry_attempt_histogram: Option<EnvoyHistogramId>,
}

#[derive(Debug, Clone)]
pub struct FilterConfig {
    config: Arc<FilterConfigImpl>,
}

impl FilterConfig {
    pub fn new(filter_config: &str) -> Option<Self> {
        let raw = Self::parse_raw(filter_config)?;
        Some(Self::from_raw(raw, None, None, None))
    }

    pub fn new_with_env<EC: EnvoyHttpFilterConfig>(
        envoy_filter_config: &mut EC,
        filter_config: &str,
    ) -> Option<Self> {
        let raw = Self::parse_raw(filter_config)?;
        let requests_total_counter =
            match envoy_filter_config.define_counter("focus_guard.requests_total") {
                Ok(id) => Some(id),
                Err(_) => {
                    envoy_log_warn!("focus-guard: failed to define requests_total counter");
                    None
                }
            };
        let throttled_total_counter =
            match envoy_filter_config.define_counter("focus_guard.throttled_total") {
                Ok(id) => Some(id),
                Err(_) => {
                    envoy_log_warn!("focus-guard: failed to define throttled_total counter");
                    None
                }
            };
        let retry_attempt_histogram =
            match envoy_filter_config.define_histogram("focus_guard.retry_attempt") {
                Ok(id) => Some(id),
                Err(_) => {
                    envoy_log_warn!("focus-guard: failed to define retry_attempt histogram");
                    None
                }
            };

        Some(Self::from_raw(
            raw,
            requests_total_counter,
            throttled_total_counter,
            retry_attempt_histogram,
        ))
    }

    fn parse_raw(filter_config: &str) -> Option<RawFilterConfig> {
        if filter_config.is_empty() {
            Some(RawFilterConfig::default())
        } else {
            match serde_json::from_str(filter_config) {
                Ok(cfg) => Some(cfg),
                Err(err) => {
                    eprintln!("Error parsing filter config: {err}");
                    None
                }
            }
        }
    }

    fn from_raw(
        raw: RawFilterConfig,
        requests_total_counter: Option<EnvoyCounterId>,
        throttled_total_counter: Option<EnvoyCounterId>,
        retry_attempt_histogram: Option<EnvoyHistogramId>,
    ) -> Self {
        let retry_threshold = raw.retry_threshold.max(1);
        let overload_status_code = if (100..=599).contains(&raw.overload_status_code) {
            raw.overload_status_code
        } else {
            DEFAULT_OVERLOAD_STATUS_CODE
        };

        FilterConfig {
            config: Arc::new(FilterConfigImpl {
                retry_threshold,
                overload_status_code,
                overload_body: raw.overload_body,
                enable_tars: raw.enable_tars,
                tars_cluster: raw.tars_cluster,
                tars_authority: raw.tars_authority,
                tars_path: if raw.tars_path.starts_with('/') {
                    raw.tars_path
                } else {
                    format!("/{}", raw.tars_path)
                },
                tars_model: raw.tars_model,
                tars_timeout_milliseconds: raw.tars_timeout_milliseconds.max(1),
                tars_fail_open: raw.tars_fail_open,
                tars_api_key: raw.tars_api_key,
                requests_total_counter,
                throttled_total_counter,
                retry_attempt_histogram,
            }),
        }
    }
}

// The per-route filter config that can override the global config for specific routes.
#[derive(Debug, Clone)]
pub struct PerRouteFilterConfig {
    config: Arc<FilterConfigImpl>,
}

impl PerRouteFilterConfig {
    pub fn create(_name: &str, config: &[u8]) -> Option<Box<dyn Any>> {
        if config.is_empty() {
            return None;
        }
        let config_str = std::str::from_utf8(config).unwrap_or("");
        FilterConfig::new(config_str)
            .map(|fc| Box::new(PerRouteFilterConfig { config: fc.config }) as Box<dyn Any>)
    }
}

fn merge_per_route_with_global_metrics(
    per_route: &Arc<FilterConfigImpl>,
    global: &Arc<FilterConfigImpl>,
) -> Arc<FilterConfigImpl> {
    Arc::new(FilterConfigImpl {
        retry_threshold: per_route.retry_threshold,
        overload_status_code: per_route.overload_status_code,
        overload_body: per_route.overload_body.clone(),
        enable_tars: per_route.enable_tars,
        tars_cluster: per_route.tars_cluster.clone(),
        tars_authority: per_route.tars_authority.clone(),
        tars_path: per_route.tars_path.clone(),
        tars_model: per_route.tars_model.clone(),
        tars_timeout_milliseconds: per_route.tars_timeout_milliseconds,
        tars_fail_open: per_route.tars_fail_open,
        tars_api_key: per_route.tars_api_key.clone(),
        requests_total_counter: global.requests_total_counter.clone(),
        throttled_total_counter: global.throttled_total_counter.clone(),
        retry_attempt_histogram: global.retry_attempt_histogram.clone(),
    })
}

impl<EHF: EnvoyHttpFilter> HttpFilterConfig<EHF> for FilterConfig {
    fn new_http_filter(&self, envoy: &mut EHF) -> Box<dyn HttpFilter<EHF>> {
        let config = if let Some(route_cfg) = envoy.get_most_specific_route_config() {
            if let Some(per_route) = route_cfg.downcast_ref::<PerRouteFilterConfig>() {
                merge_per_route_with_global_metrics(&per_route.config, &self.config)
            } else {
                self.config.clone()
            }
        } else {
            self.config.clone()
        };
        Box::new(Filter {
            config,
            last_retry_attempt: 1,
            throttled: false,
            pending_tars_callout_id: None,
        })
    }
}

pub struct Filter {
    config: Arc<FilterConfigImpl>,
    last_retry_attempt: u32,
    throttled: bool,
    pending_tars_callout_id: Option<u64>,
}

impl Filter {
    fn parse_retry_attempt<EHF: EnvoyHttpFilter>(envoy_filter: &EHF) -> u32 {
        envoy_filter
            .get_request_header_value("x-envoy-attempt-count")
            .and_then(|header| {
                std::str::from_utf8(header.as_slice())
                    .ok()
                    .and_then(|value| value.trim().parse::<u32>().ok())
            })
            .filter(|attempt| *attempt > 0)
            .unwrap_or(1)
    }

    fn tars_header_value(&self) -> &'static [u8] {
        if self.config.enable_tars {
            b"active"
        } else {
            b"disabled"
        }
    }

    fn request_header_as_string<EHF: EnvoyHttpFilter>(envoy_filter: &EHF, key: &str) -> String {
        envoy_filter
            .get_request_header_value(key)
            .and_then(|value| {
                std::str::from_utf8(value.as_slice())
                    .ok()
                    .map(str::to_string)
            })
            .unwrap_or_default()
    }

    fn build_tars_payload<EHF: EnvoyHttpFilter>(&self, envoy_filter: &EHF) -> Option<Vec<u8>> {
        let method = Self::request_header_as_string(envoy_filter, ":method");
        let path = Self::request_header_as_string(envoy_filter, ":path");
        let authority = Self::request_header_as_string(envoy_filter, ":authority");

        let user_content = format!(
            "You are Focus Guard. Return strict JSON: {{\"decision\":\"pass|throttle\",\"reason\":\"short reason\"}}. \
             Evaluate this request context and decide whether to throttle.\n\
             retry_attempt={}\nretry_threshold={}\nmethod={}\npath={}\nauthority={}",
            self.last_retry_attempt,
            self.config.retry_threshold,
            method,
            path,
            authority
        );

        let payload = serde_json::json!({
            "model": self.config.tars_model,
            "response_format": {"type": "json_object"},
            "messages": [
                {
                    "role": "system",
                    "content": "You make proxy traffic decisions for request throttling."
                },
                {
                    "role": "user",
                    "content": user_content
                }
            ]
        });

        serde_json::to_vec(&payload).ok()
    }

    fn maybe_start_tars_callout<EHF: EnvoyHttpFilter>(&mut self, envoy_filter: &mut EHF) -> bool {
        let Some(payload) = self.build_tars_payload(envoy_filter) else {
            envoy_log_warn!("focus-guard: failed to build TARS payload");
            return false;
        };
        let mut owned_headers: Vec<(&str, Vec<u8>)> = vec![
            (":method", b"POST".to_vec()),
            (":path", self.config.tars_path.as_bytes().to_vec()),
            ("host", self.config.tars_authority.as_bytes().to_vec()),
            (":authority", self.config.tars_authority.as_bytes().to_vec()),
            ("content-type", b"application/json".to_vec()),
            ("content-length", payload.len().to_string().into_bytes()),
        ];

        if let Some(api_key) = self.config.tars_api_key.as_ref() {
            owned_headers.push(("authorization", format!("Bearer {}", api_key).into_bytes()));
        }

        let header_refs: Vec<(&str, &[u8])> = owned_headers
            .iter()
            .map(|(key, value)| (*key, value.as_slice()))
            .collect();

        let (result, callout_id) = envoy_filter.send_http_callout(
            &self.config.tars_cluster,
            &header_refs,
            Some(payload.as_slice()),
            self.config.tars_timeout_milliseconds,
        );

        if result == abi::envoy_dynamic_module_type_http_callout_init_result::Success {
            self.pending_tars_callout_id = Some(callout_id);
            envoy_filter.add_custom_flag("focus_guard_tars_active");
            true
        } else {
            envoy_log_warn!(
                "focus-guard: failed to initialize TARS callout, result={}",
                result as u32
            );
            false
        }
    }

    fn parse_decision_keyword(value: &str) -> Option<bool> {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized.contains("throttle")
            || normalized.contains("block")
            || normalized.contains("deny")
            || normalized.contains("reject")
        {
            return Some(true);
        }
        if normalized.contains("pass")
            || normalized.contains("allow")
            || normalized.contains("continue")
        {
            return Some(false);
        }
        None
    }

    fn parse_decision_from_json(value: &Value) -> Option<(bool, Option<String>)> {
        if let Some(decision_value) = value.get("decision").and_then(Value::as_str) {
            let reason = value
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(throttle) = Self::parse_decision_keyword(decision_value) {
                return Some((throttle, reason));
            }
        }

        if let Some(content) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("message"))
            .and_then(|message| message.get("content"))
            .and_then(Value::as_str)
        {
            if let Ok(inner_json) = serde_json::from_str::<Value>(content) {
                if let Some(parsed) = Self::parse_decision_from_json(&inner_json) {
                    return Some(parsed);
                }
            }

            if let Some(throttle) = Self::parse_decision_keyword(content) {
                return Some((throttle, Some(content.to_string())));
            }
        }

        None
    }

    fn parse_tars_decision(
        response_body: Option<&[EnvoyBuffer]>,
    ) -> Option<(bool, Option<String>)> {
        let chunks = response_body?;
        let mut bytes = Vec::new();
        for chunk in chunks {
            bytes.extend_from_slice(chunk.as_slice());
        }

        if bytes.is_empty() {
            return None;
        }

        let body_str = String::from_utf8_lossy(&bytes);
        if let Ok(json_value) = serde_json::from_slice::<Value>(&bytes) {
            if let Some(parsed) = Self::parse_decision_from_json(&json_value) {
                return Some(parsed);
            }
        }

        Self::parse_decision_keyword(body_str.as_ref())
            .map(|throttle| (throttle, Some(body_str.into_owned())))
    }

    fn mark_throttled<EHF: EnvoyHttpFilter>(&mut self, envoy_filter: &mut EHF) {
        self.throttled = true;

        if let Some(counter_id) = self.config.throttled_total_counter.clone() {
            if envoy_filter.increment_counter(counter_id, 1).is_err() {
                envoy_log_warn!("focus-guard: failed to increment throttled_total");
            }
        }

        envoy_filter.set_dynamic_metadata_bool(DYNAMIC_METADATA_NAMESPACE, "throttled", true);
        envoy_filter.set_request_header("x-focus-guard", b"throttled");
    }

    fn mark_pass<EHF: EnvoyHttpFilter>(&mut self, envoy_filter: &mut EHF) {
        self.throttled = false;
        envoy_filter.set_dynamic_metadata_bool(DYNAMIC_METADATA_NAMESPACE, "throttled", false);
        envoy_filter.set_request_header("x-focus-guard", b"pass");
    }

    fn send_local_throttle_response<EHF: EnvoyHttpFilter>(
        &self,
        envoy_filter: &mut EHF,
        details: &'static str,
    ) {
        let retry_attempt_header = self.last_retry_attempt.to_string();
        let headers = [
            ("x-focus-guard", b"throttled".as_slice()),
            (
                "x-focus-guard-retry-attempt",
                retry_attempt_header.as_bytes(),
            ),
            ("x-focus-guard-tars", self.tars_header_value()),
            ("content-type", b"text/plain; charset=utf-8".as_slice()),
        ];
        envoy_filter.send_response(
            self.config.overload_status_code,
            &headers,
            Some(self.config.overload_body.as_bytes()),
            Some(details),
        );
    }

    fn handle_tars_unavailable<EHF: EnvoyHttpFilter>(
        &mut self,
        envoy_filter: &mut EHF,
        detail: &'static str,
    ) -> abi::envoy_dynamic_module_type_on_http_filter_request_headers_status {
        if self.config.tars_fail_open {
            envoy_filter.add_custom_flag("focus_guard_tars_fail_open");
            self.mark_pass(envoy_filter);
            abi::envoy_dynamic_module_type_on_http_filter_request_headers_status::Continue
        } else {
            envoy_filter.add_custom_flag("focus_guard_tars_fail_closed");
            self.mark_throttled(envoy_filter);
            self.send_local_throttle_response(envoy_filter, detail);
            abi::envoy_dynamic_module_type_on_http_filter_request_headers_status::StopIteration
        }
    }
}

impl<EHF: EnvoyHttpFilter> HttpFilter<EHF> for Filter {
    fn on_request_headers(
        &mut self,
        envoy_filter: &mut EHF,
        _end_stream: bool,
    ) -> abi::envoy_dynamic_module_type_on_http_filter_request_headers_status {
        self.pending_tars_callout_id = None;
        self.last_retry_attempt = Self::parse_retry_attempt(envoy_filter);

        if let Some(counter_id) = self.config.requests_total_counter.clone() {
            if envoy_filter.increment_counter(counter_id, 1).is_err() {
                envoy_log_warn!("focus-guard: failed to increment requests_total");
            }
        }

        if let Some(histogram_id) = self.config.retry_attempt_histogram.clone() {
            if envoy_filter
                .record_histogram_value(histogram_id, self.last_retry_attempt as u64)
                .is_err()
            {
                envoy_log_warn!("focus-guard: failed to record retry_attempt histogram");
            }
        }

        envoy_filter.set_dynamic_metadata_number(
            DYNAMIC_METADATA_NAMESPACE,
            "retry_attempt",
            self.last_retry_attempt as f64,
        );

        if self.last_retry_attempt >= self.config.retry_threshold {
            self.mark_throttled(envoy_filter);
            self.send_local_throttle_response(envoy_filter, "focus_guard_retry_threshold");
            envoy_log_warn!(
                "focus-guard: throttled request at attempt={} threshold={}",
                self.last_retry_attempt,
                self.config.retry_threshold
            );
            return abi::envoy_dynamic_module_type_on_http_filter_request_headers_status::StopIteration;
        }

        if self.config.enable_tars {
            if self.maybe_start_tars_callout(envoy_filter) {
                envoy_filter.set_dynamic_metadata_string(
                    DYNAMIC_METADATA_NAMESPACE,
                    "tars_decision",
                    "pending",
                );
                return abi::envoy_dynamic_module_type_on_http_filter_request_headers_status::StopAllIterationAndBuffer;
            }

            return self
                .handle_tars_unavailable(envoy_filter, "focus_guard_tars_callout_init_failed");
        }

        self.mark_pass(envoy_filter);
        abi::envoy_dynamic_module_type_on_http_filter_request_headers_status::Continue
    }

    fn on_request_body(
        &mut self,
        _envoy_filter: &mut EHF,
        _end_stream: bool,
    ) -> abi::envoy_dynamic_module_type_on_http_filter_request_body_status {
        abi::envoy_dynamic_module_type_on_http_filter_request_body_status::Continue
    }

    fn on_request_trailers(
        &mut self,
        _envoy_filter: &mut EHF,
    ) -> abi::envoy_dynamic_module_type_on_http_filter_request_trailers_status {
        abi::envoy_dynamic_module_type_on_http_filter_request_trailers_status::Continue
    }

    fn on_http_callout_done(
        &mut self,
        envoy_filter: &mut EHF,
        callout_id: u64,
        result: abi::envoy_dynamic_module_type_http_callout_result,
        _response_headers: Option<&[(EnvoyBuffer, EnvoyBuffer)]>,
        response_body: Option<&[EnvoyBuffer]>,
    ) {
        if self.pending_tars_callout_id != Some(callout_id) {
            return;
        }
        self.pending_tars_callout_id = None;

        if result != abi::envoy_dynamic_module_type_http_callout_result::Success {
            envoy_log_warn!(
                "focus-guard: TARS callout failed, result={}, fail_open={}",
                result as u32,
                self.config.tars_fail_open
            );
            envoy_filter.set_dynamic_metadata_string(
                DYNAMIC_METADATA_NAMESPACE,
                "tars_decision",
                "callout_failed",
            );
            if self.config.tars_fail_open {
                self.mark_pass(envoy_filter);
                envoy_filter.continue_decoding();
            } else {
                self.mark_throttled(envoy_filter);
                self.send_local_throttle_response(envoy_filter, "focus_guard_tars_callout_failed");
            }
            return;
        }

        match Self::parse_tars_decision(response_body) {
            Some((true, reason)) => {
                self.mark_throttled(envoy_filter);
                envoy_filter.set_dynamic_metadata_string(
                    DYNAMIC_METADATA_NAMESPACE,
                    "tars_decision",
                    "throttle",
                );
                if let Some(reason) = reason.as_deref() {
                    envoy_filter.set_dynamic_metadata_string(
                        DYNAMIC_METADATA_NAMESPACE,
                        "tars_reason",
                        reason,
                    );
                }
                self.send_local_throttle_response(envoy_filter, "focus_guard_tars_decision");
            }
            Some((false, reason)) => {
                self.mark_pass(envoy_filter);
                envoy_filter.set_dynamic_metadata_string(
                    DYNAMIC_METADATA_NAMESPACE,
                    "tars_decision",
                    "pass",
                );
                if let Some(reason) = reason.as_deref() {
                    envoy_filter.set_dynamic_metadata_string(
                        DYNAMIC_METADATA_NAMESPACE,
                        "tars_reason",
                        reason,
                    );
                }
                envoy_filter.continue_decoding();
            }
            None => {
                envoy_log_warn!(
                    "focus-guard: TARS response could not be parsed, fail_open={}",
                    self.config.tars_fail_open
                );
                envoy_filter.set_dynamic_metadata_string(
                    DYNAMIC_METADATA_NAMESPACE,
                    "tars_decision",
                    "unparseable",
                );
                if self.config.tars_fail_open {
                    self.mark_pass(envoy_filter);
                    envoy_filter.continue_decoding();
                } else {
                    self.mark_throttled(envoy_filter);
                    self.send_local_throttle_response(
                        envoy_filter,
                        "focus_guard_tars_invalid_response",
                    );
                }
            }
        }
    }

    fn on_response_headers(
        &mut self,
        envoy_filter: &mut EHF,
        _end_stream: bool,
    ) -> abi::envoy_dynamic_module_type_on_http_filter_response_headers_status {
        let decision: &[u8] = if self.throttled {
            b"throttled"
        } else {
            b"pass"
        };
        let tars_mode = self.tars_header_value();
        let retry_attempt = self.last_retry_attempt.to_string();

        envoy_filter.set_response_header("x-focus-guard", decision);
        envoy_filter.set_response_header("x-focus-guard-retry-attempt", retry_attempt.as_bytes());
        envoy_filter.set_response_header("x-focus-guard-tars", tars_mode);

        abi::envoy_dynamic_module_type_on_http_filter_response_headers_status::Continue
    }

    fn on_response_body(
        &mut self,
        _envoy_filter: &mut EHF,
        _end_stream: bool,
    ) -> abi::envoy_dynamic_module_type_on_http_filter_response_body_status {
        abi::envoy_dynamic_module_type_on_http_filter_response_body_status::Continue
    }

    fn on_response_trailers(
        &mut self,
        _envoy_filter: &mut EHF,
    ) -> abi::envoy_dynamic_module_type_on_http_filter_response_trailers_status {
        abi::envoy_dynamic_module_type_on_http_filter_response_trailers_status::Continue
    }
    fn on_stream_complete(&mut self, _envoy_filter: &mut EHF) {
        self.pending_tars_callout_id = None;
    }
}

fn init() -> bool {
    true
}

#[allow(dead_code)]
fn new_http_filter_config_fn<EC: EnvoyHttpFilterConfig, EHF: EnvoyHttpFilter>(
    envoy_filter_config: &mut EC,
    filter_name: &str,
    filter_config: &[u8],
) -> Option<Box<dyn HttpFilterConfig<EHF>>> {
    let filter_config = std::str::from_utf8(filter_config).unwrap_or("");
    match filter_name {
        "focus-guard" => FilterConfig::new_with_env(envoy_filter_config, filter_config)
            .map(|config| Box::new(config) as Box<dyn HttpFilterConfig<EHF>>),
        _ => panic!("Unknown filter name: {filter_name}"),
    }
}

#[allow(dead_code)]
fn new_http_filter_per_route_config_fn(name: &str, config: &[u8]) -> Option<Box<dyn Any>> {
    match name {
        "focus-guard" => PerRouteFilterConfig::create(name, config),
        _ => panic!("Unknown filter name: {name}"),
    }
}

declare_init_functions!(
    init,
    new_http_filter_config_fn,
    new_http_filter_per_route_config_fn
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_filter_config_defaults() {
        let filter_config = FilterConfig::new("");
        assert!(filter_config.is_some());
        let filter_config = filter_config.unwrap();
        assert_eq!(
            filter_config.config.retry_threshold,
            DEFAULT_RETRY_THRESHOLD
        );
        assert_eq!(
            filter_config.config.overload_status_code,
            DEFAULT_OVERLOAD_STATUS_CODE
        );
        assert_eq!(filter_config.config.overload_body, DEFAULT_OVERLOAD_BODY);
        assert!(!filter_config.config.enable_tars);
        assert_eq!(filter_config.config.tars_cluster, DEFAULT_TARS_CLUSTER);
        assert_eq!(filter_config.config.tars_authority, DEFAULT_TARS_AUTHORITY);
        assert_eq!(filter_config.config.tars_path, DEFAULT_TARS_PATH);
        assert_eq!(filter_config.config.tars_model, DEFAULT_TARS_MODEL);
        assert_eq!(
            filter_config.config.tars_timeout_milliseconds,
            DEFAULT_TARS_TIMEOUT_MILLISECONDS
        );
        assert_eq!(filter_config.config.tars_fail_open, DEFAULT_TARS_FAIL_OPEN);
        assert!(filter_config.config.tars_api_key.is_none());
        assert!(filter_config.config.requests_total_counter.is_none());
        assert!(filter_config.config.throttled_total_counter.is_none());
        assert!(filter_config.config.retry_attempt_histogram.is_none());
    }

    #[test]
    fn test_new_filter_config_custom_values() {
        let filter_config = FilterConfig::new(
            r#"{
                "retry_threshold": 5,
                "overload_status_code": 503,
                "overload_body": "slow down",
                "enable_tars": true,
                "tars_cluster": "tars-cluster",
                "tars_authority": "api.example.com",
                "tars_path": "v1/decision",
                "tars_model": "gpt-test",
                "tars_timeout_milliseconds": 600,
                "tars_fail_open": false,
                "tars_api_key": "secret-key"
            }"#,
        );
        assert!(filter_config.is_some());
        let filter_config = filter_config.unwrap();
        assert_eq!(filter_config.config.retry_threshold, 5);
        assert_eq!(filter_config.config.overload_status_code, 503);
        assert_eq!(filter_config.config.overload_body, "slow down");
        assert!(filter_config.config.enable_tars);
        assert_eq!(filter_config.config.tars_cluster, "tars-cluster");
        assert_eq!(filter_config.config.tars_authority, "api.example.com");
        assert_eq!(filter_config.config.tars_path, "/v1/decision");
        assert_eq!(filter_config.config.tars_model, "gpt-test");
        assert_eq!(filter_config.config.tars_timeout_milliseconds, 600);
        assert!(!filter_config.config.tars_fail_open);
        assert_eq!(
            filter_config.config.tars_api_key.as_deref(),
            Some("secret-key")
        );
    }

    #[test]
    fn test_new_filter_config_invalid_values_fallback() {
        let filter_config = FilterConfig::new(
            r#"{
                "retry_threshold": 0,
                "overload_status_code": 700
            }"#,
        );
        assert!(filter_config.is_some());
        let filter_config = filter_config.unwrap();
        assert_eq!(filter_config.config.retry_threshold, 1);
        assert_eq!(
            filter_config.config.overload_status_code,
            DEFAULT_OVERLOAD_STATUS_CODE
        );
    }

    #[test]
    fn test_new_filter_config_invalid_json() {
        let filter_config = FilterConfig::new("{invalid");
        assert!(filter_config.is_none());
    }

    #[test]
    fn test_new_per_route_filter_config_empty() {
        let result = PerRouteFilterConfig::create("focus-guard", b"");
        assert!(result.is_none());
    }

    #[test]
    fn test_new_per_route_filter_config_invalid_json() {
        let result = PerRouteFilterConfig::create("focus-guard", b"{invalid");
        assert!(result.is_none());
    }

    #[test]
    fn test_on_request_headers_continue_when_below_threshold() {
        let filter_config = FilterConfig::new(r#"{"retry_threshold": 2}"#).unwrap();
        let mut filter = Filter {
            config: filter_config.config,
            last_retry_attempt: 0,
            throttled: false,
            pending_tars_callout_id: None,
        };

        let mut mock_envoy_filter =
            envoy_proxy_dynamic_modules_rust_sdk::MockEnvoyHttpFilter::new();
        mock_envoy_filter
            .expect_get_request_header_value()
            .times(1)
            .returning(|key| {
                assert_eq!(key, "x-envoy-attempt-count");
                None
            });
        mock_envoy_filter
            .expect_set_dynamic_metadata_number()
            .times(1)
            .returning(|namespace, key, value| {
                assert_eq!(namespace, DYNAMIC_METADATA_NAMESPACE);
                assert_eq!(key, "retry_attempt");
                assert_eq!(value, 1.0);
            });
        mock_envoy_filter
            .expect_set_dynamic_metadata_bool()
            .times(1)
            .returning(|namespace, key, value| {
                assert_eq!(namespace, DYNAMIC_METADATA_NAMESPACE);
                assert_eq!(key, "throttled");
                assert!(!value);
            });
        mock_envoy_filter
            .expect_set_request_header()
            .times(1)
            .returning(|key, value| {
                assert_eq!(key, "x-focus-guard");
                assert_eq!(value, b"pass");
                true
            });

        assert_eq!(
            filter.on_request_headers(&mut mock_envoy_filter, false),
            abi::envoy_dynamic_module_type_on_http_filter_request_headers_status::Continue
        );
        assert_eq!(filter.last_retry_attempt, 1);
        assert!(!filter.throttled);
    }

    #[test]
    fn test_on_request_headers_throttle_at_threshold() {
        let filter_config = FilterConfig::new(r#"{"retry_threshold": 1}"#).unwrap();
        let mut filter = Filter {
            config: filter_config.config,
            last_retry_attempt: 0,
            throttled: false,
            pending_tars_callout_id: None,
        };

        let mut mock_envoy_filter =
            envoy_proxy_dynamic_modules_rust_sdk::MockEnvoyHttpFilter::new();
        mock_envoy_filter
            .expect_get_request_header_value()
            .times(1)
            .returning(|_| None);
        mock_envoy_filter
            .expect_set_dynamic_metadata_number()
            .times(1)
            .returning(|_, _, _| ());
        mock_envoy_filter
            .expect_set_dynamic_metadata_bool()
            .times(1)
            .returning(|namespace, key, value| {
                assert_eq!(namespace, DYNAMIC_METADATA_NAMESPACE);
                assert_eq!(key, "throttled");
                assert!(value);
            });
        mock_envoy_filter
            .expect_set_request_header()
            .times(1)
            .returning(|key, value| {
                assert_eq!(key, "x-focus-guard");
                assert_eq!(value, b"throttled");
                true
            });
        mock_envoy_filter.expect_send_response().times(1).returning(
            |status_code, headers, body, details| {
                assert_eq!(status_code, DEFAULT_OVERLOAD_STATUS_CODE);
                assert_eq!(details, Some("focus_guard_retry_threshold"));
                assert_eq!(body, Some(DEFAULT_OVERLOAD_BODY.as_bytes()));

                let mut saw_focus_header = false;
                let mut saw_retry_attempt_header = false;
                let mut saw_tars_header = false;
                for (key, value) in headers {
                    if *key == "x-focus-guard" {
                        assert_eq!(*value, b"throttled");
                        saw_focus_header = true;
                    }
                    if *key == "x-focus-guard-retry-attempt" {
                        assert_eq!(*value, b"1");
                        saw_retry_attempt_header = true;
                    }
                    if *key == "x-focus-guard-tars" {
                        assert_eq!(*value, b"disabled");
                        saw_tars_header = true;
                    }
                }
                assert!(saw_focus_header);
                assert!(saw_retry_attempt_header);
                assert!(saw_tars_header);
            },
        );

        assert_eq!(
            filter.on_request_headers(&mut mock_envoy_filter, false),
            abi::envoy_dynamic_module_type_on_http_filter_request_headers_status::StopIteration
        );
        assert_eq!(filter.last_retry_attempt, 1);
        assert!(filter.throttled);
    }

    #[test]
    fn test_parse_tars_decision_from_openai_json_content() {
        let payload = br#"{
            "choices": [
                {
                    "message": {
                        "content": "{\"decision\":\"throttle\",\"reason\":\"high retry pressure\"}"
                    }
                }
            ]
        }"#;
        let chunks = [EnvoyBuffer::new(payload)];
        let parsed = Filter::parse_tars_decision(Some(&chunks));
        assert_eq!(parsed.as_ref().map(|(throttle, _)| *throttle), Some(true));
        assert_eq!(
            parsed.as_ref().and_then(|(_, reason)| reason.as_deref()),
            Some("high retry pressure")
        );
    }

    #[test]
    fn test_on_request_headers_tars_enabled_starts_callout() {
        let filter_config = FilterConfig::new(r#"{"enable_tars": true}"#).unwrap();
        let mut filter = Filter {
            config: filter_config.config,
            last_retry_attempt: 0,
            throttled: false,
            pending_tars_callout_id: None,
        };

        let mut mock_envoy_filter =
            envoy_proxy_dynamic_modules_rust_sdk::MockEnvoyHttpFilter::new();
        mock_envoy_filter
            .expect_get_request_header_value()
            .times(4)
            .returning(|key| {
                if key == "x-envoy-attempt-count" {
                    Some(EnvoyBuffer::new(b"1"))
                } else {
                    None
                }
            });
        mock_envoy_filter
            .expect_set_dynamic_metadata_number()
            .times(1)
            .returning(|_, _, _| ());
        mock_envoy_filter
            .expect_send_http_callout()
            .times(1)
            .returning(|cluster, headers, body, timeout| {
                assert_eq!(cluster, DEFAULT_TARS_CLUSTER);
                assert_eq!(timeout, DEFAULT_TARS_TIMEOUT_MILLISECONDS);
                assert!(body.is_some());
                let body = body.unwrap();
                assert!(!body.is_empty());
                let has_method = headers
                    .iter()
                    .any(|(k, v)| *k == ":method" && *v == b"POST");
                let has_path = headers
                    .iter()
                    .any(|(k, v)| *k == ":path" && *v == DEFAULT_TARS_PATH.as_bytes());
                assert!(has_method);
                assert!(has_path);
                (
                    abi::envoy_dynamic_module_type_http_callout_init_result::Success,
                    77,
                )
            });
        mock_envoy_filter
            .expect_add_custom_flag()
            .times(1)
            .returning(|flag| assert_eq!(flag, "focus_guard_tars_active"));
        mock_envoy_filter
            .expect_set_dynamic_metadata_string()
            .times(1)
            .returning(|namespace, key, value| {
                assert_eq!(namespace, DYNAMIC_METADATA_NAMESPACE);
                assert_eq!(key, "tars_decision");
                assert_eq!(value, "pending");
            });

        assert_eq!(
            filter.on_request_headers(&mut mock_envoy_filter, false),
            abi::envoy_dynamic_module_type_on_http_filter_request_headers_status::StopAllIterationAndBuffer
        );
        assert_eq!(filter.pending_tars_callout_id, Some(77));
        assert!(!filter.throttled);
    }

    #[test]
    fn test_on_http_callout_done_fail_open_continues_request() {
        let filter_config =
            FilterConfig::new(r#"{"enable_tars": true, "tars_fail_open": true}"#).unwrap();
        let mut filter = Filter {
            config: filter_config.config,
            last_retry_attempt: 2,
            throttled: false,
            pending_tars_callout_id: Some(99),
        };

        let mut mock_envoy_filter =
            envoy_proxy_dynamic_modules_rust_sdk::MockEnvoyHttpFilter::new();
        mock_envoy_filter
            .expect_set_dynamic_metadata_string()
            .times(1)
            .returning(|namespace, key, value| {
                assert_eq!(namespace, DYNAMIC_METADATA_NAMESPACE);
                assert_eq!(key, "tars_decision");
                assert_eq!(value, "callout_failed");
            });
        mock_envoy_filter
            .expect_set_dynamic_metadata_bool()
            .times(1)
            .returning(|namespace, key, value| {
                assert_eq!(namespace, DYNAMIC_METADATA_NAMESPACE);
                assert_eq!(key, "throttled");
                assert!(!value);
            });
        mock_envoy_filter
            .expect_set_request_header()
            .times(1)
            .returning(|key, value| {
                assert_eq!(key, "x-focus-guard");
                assert_eq!(value, b"pass");
                true
            });
        mock_envoy_filter
            .expect_continue_decoding()
            .times(1)
            .returning(|| ());

        filter.on_http_callout_done(
            &mut mock_envoy_filter,
            99,
            abi::envoy_dynamic_module_type_http_callout_result::Reset,
            None,
            None,
        );

        assert_eq!(filter.pending_tars_callout_id, None);
        assert!(!filter.throttled);
    }

    #[test]
    fn test_on_http_callout_done_throttles_on_tars_decision() {
        let filter_config = FilterConfig::new(r#"{"enable_tars": true}"#).unwrap();
        let mut filter = Filter {
            config: filter_config.config,
            last_retry_attempt: 2,
            throttled: false,
            pending_tars_callout_id: Some(101),
        };

        let mut mock_envoy_filter =
            envoy_proxy_dynamic_modules_rust_sdk::MockEnvoyHttpFilter::new();
        mock_envoy_filter
            .expect_set_dynamic_metadata_bool()
            .times(1)
            .returning(|namespace, key, value| {
                assert_eq!(namespace, DYNAMIC_METADATA_NAMESPACE);
                assert_eq!(key, "throttled");
                assert!(value);
            });
        mock_envoy_filter
            .expect_set_request_header()
            .times(1)
            .returning(|key, value| {
                assert_eq!(key, "x-focus-guard");
                assert_eq!(value, b"throttled");
                true
            });
        mock_envoy_filter
            .expect_set_dynamic_metadata_string()
            .times(2)
            .returning(|namespace, key, value| {
                assert_eq!(namespace, DYNAMIC_METADATA_NAMESPACE);
                if key == "tars_decision" {
                    assert_eq!(value, "throttle");
                } else {
                    assert_eq!(key, "tars_reason");
                    assert_eq!(value, "model says block");
                }
            });
        mock_envoy_filter.expect_send_response().times(1).returning(
            |status_code, headers, body, details| {
                assert_eq!(status_code, DEFAULT_OVERLOAD_STATUS_CODE);
                assert_eq!(details, Some("focus_guard_tars_decision"));
                assert_eq!(body, Some(DEFAULT_OVERLOAD_BODY.as_bytes()));
                let mut saw_retry_header = false;
                let mut saw_tars_header = false;
                for (key, value) in headers {
                    if *key == "x-focus-guard-retry-attempt" {
                        assert_eq!(*value, b"2");
                        saw_retry_header = true;
                    }
                    if *key == "x-focus-guard-tars" {
                        assert_eq!(*value, b"active");
                        saw_tars_header = true;
                    }
                }
                assert!(saw_retry_header);
                assert!(saw_tars_header);
            },
        );

        let response_payload = br#"{"decision":"throttle","reason":"model says block"}"#;
        let response_chunks = [EnvoyBuffer::new(response_payload)];
        filter.on_http_callout_done(
            &mut mock_envoy_filter,
            101,
            abi::envoy_dynamic_module_type_http_callout_result::Success,
            None,
            Some(&response_chunks),
        );

        assert_eq!(filter.pending_tars_callout_id, None);
        assert!(filter.throttled);
    }

    #[test]
    fn test_on_response_headers_sets_focus_headers() {
        let filter_config = FilterConfig::new("").unwrap();
        let mut filter = Filter {
            config: filter_config.config,
            last_retry_attempt: 4,
            throttled: true,
            pending_tars_callout_id: None,
        };

        let mut mock_envoy_filter =
            envoy_proxy_dynamic_modules_rust_sdk::MockEnvoyHttpFilter::new();
        mock_envoy_filter
            .expect_set_response_header()
            .times(3)
            .returning(|key, value| {
                match key {
                    "x-focus-guard" => assert_eq!(value, b"throttled"),
                    "x-focus-guard-retry-attempt" => assert_eq!(value, b"4"),
                    "x-focus-guard-tars" => assert_eq!(value, b"disabled"),
                    _ => panic!("unexpected response header key: {key}"),
                }
                true
            });

        assert_eq!(
            filter.on_response_headers(&mut mock_envoy_filter, false),
            abi::envoy_dynamic_module_type_on_http_filter_response_headers_status::Continue
        );
    }
}
