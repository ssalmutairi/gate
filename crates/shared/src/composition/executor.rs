use crate::composition::dag::compute_levels;

/// Validate a request body against a JSON Schema (lightweight, no external crate).
/// Checks `type`, `required`, and property types (string/number/integer/boolean/array/object).
pub fn validate_input_schema(schema: &Value, body: &Option<Value>) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();

    let schema_type = schema.get("type").and_then(|t| t.as_str()).unwrap_or("object");
    if schema_type != "object" {
        return Ok(());
    }

    let body_obj = match body {
        Some(Value::Object(map)) => map,
        Some(_) => {
            errors.push("request body must be a JSON object".to_string());
            return Err(errors);
        }
        None => {
            // Check if there are required fields
            if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                if !required.is_empty() {
                    errors.push("request body is required".to_string());
                    return Err(errors);
                }
            }
            return Ok(());
        }
    };

    // Check required fields
    if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
        for req in required {
            if let Some(field_name) = req.as_str() {
                if !body_obj.contains_key(field_name) {
                    errors.push(format!("missing required field '{}'", field_name));
                }
            }
        }
    }

    // Check property types
    if let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) {
        for (prop_name, prop_schema) in properties {
            if let Some(value) = body_obj.get(prop_name) {
                if let Some(expected_type) = prop_schema.get("type").and_then(|t| t.as_str()) {
                    let type_ok = match expected_type {
                        "string" => value.is_string(),
                        "number" => value.is_number(),
                        "integer" => value.is_i64() || value.is_u64(),
                        "boolean" => value.is_boolean(),
                        "array" => value.is_array(),
                        "object" => value.is_object(),
                        "null" => value.is_null(),
                        _ => true,
                    };
                    if !type_ok {
                        errors.push(format!(
                            "field '{}' must be of type '{}', got '{}'",
                            prop_name,
                            expected_type,
                            json_type_name(value)
                        ));
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                "number"
            }
        }
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
use crate::composition::template::{
    resolve_template_string, resolve_template_value, StepResult, TemplateContext,
};
use crate::models::{Composition, CompositionStep, Target};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use uuid::Uuid;

/// Resolves a healthy target for a given upstream_id.
/// The caller provides this function so the executor doesn't depend on GatewayConfig directly.
pub type TargetResolver = Box<dyn Fn(&Uuid) -> Vec<Target> + Send + Sync>;

/// Error info for a step that failed with skip or default policy.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StepError {
    pub status: String,
    pub message: String,
}

/// Result of executing a composition.
#[derive(Debug)]
pub struct CompositionResult {
    pub body: Value,
    pub status: u16,
}

/// Execute a composition: run steps in DAG order, merge results.
///
/// `internal_route_base` — when set (e.g. `http://127.0.0.1:8080`), steps
/// with `use_internal_route = true` call through the local proxy instead of
/// resolving upstream targets directly. This enables SOAP translation,
/// rate limiting, and other route-level features.
pub async fn execute_composition(
    composition: &Composition,
    steps: &[CompositionStep],
    ctx: &mut TemplateContext,
    http_client: &reqwest::Client,
    target_resolver: &TargetResolver,
    compose_wait_override: Option<u64>,
    internal_route_base: Option<&str>,
) -> CompositionResult {
    let dag_steps: Vec<(String, Vec<String>)> = steps
        .iter()
        .map(|s| {
            let deps = s.depends_on.clone().unwrap_or_default();
            (s.name.clone(), deps)
        })
        .collect();

    let levels = compute_levels(&dag_steps);
    let step_map: HashMap<&str, &CompositionStep> =
        steps.iter().map(|s| (s.name.as_str(), s)).collect();

    let overall_timeout = Duration::from_millis(composition.timeout_ms as u64);
    let max_wait = compose_wait_override
        .map(Duration::from_millis)
        .or_else(|| composition.max_wait_ms.map(|ms| Duration::from_millis(ms as u64)));

    let deadline = max_wait.unwrap_or(overall_timeout);
    let start = std::time::Instant::now();

    let mut errors: HashMap<String, StepError> = HashMap::new();
    let mut aborted = false;

    for level in &levels {
        if aborted {
            break;
        }

        // Check deadline before starting this level
        if start.elapsed() >= deadline {
            for step_name in level {
                let step = step_map[step_name.as_str()];
                handle_step_failure(
                    step,
                    "timeout",
                    "deadline reached before step could start",
                    ctx,
                    &mut errors,
                    &mut aborted,
                );
                if aborted {
                    break;
                }
            }
            continue;
        }

        let remaining = deadline.saturating_sub(start.elapsed());

        if level.len() == 1 {
            // Single step — no need for join
            let step_name = &level[0];
            let step = step_map[step_name.as_str()];
            let step_timeout = Duration::from_millis(step.timeout_ms as u64).min(remaining);

            match execute_step(step, ctx, http_client, target_resolver, step_timeout, internal_route_base).await {
                Ok(result) => {
                    ctx.steps.insert(step.name.clone(), result);
                }
                Err(err_msg) => {
                    handle_step_failure(step, "error", &err_msg, ctx, &mut errors, &mut aborted);
                }
            }
        } else {
            // Multiple steps — run in parallel
            let futures: Vec<_> = level
                .iter()
                .map(|step_name| {
                    let step = step_map[step_name.as_str()];
                    let step_timeout = Duration::from_millis(step.timeout_ms as u64).min(remaining);
                    let ctx_clone = ctx.clone();
                    async move {
                        let result =
                            execute_step(step, &ctx_clone, http_client, target_resolver, step_timeout, internal_route_base)
                                .await;
                        (step.name.clone(), step, result)
                    }
                })
                .collect();

            let results = futures::future::join_all(futures).await;

            for (name, step, result) in results {
                match result {
                    Ok(step_result) => {
                        ctx.steps.insert(name, step_result);
                    }
                    Err(err_msg) => {
                        handle_step_failure(step, "error", &err_msg, ctx, &mut errors, &mut aborted);
                        if aborted {
                            break;
                        }
                    }
                }
            }
        }
    }

    // Mark any remaining steps not yet executed as timed out
    if aborted || start.elapsed() >= deadline {
        for level in &levels {
            for step_name in level {
                if !ctx.steps.contains_key(step_name.as_str())
                    && !errors.contains_key(step_name.as_str())
                {
                    let step = step_map[step_name.as_str()];
                    let mut dummy_aborted = false;
                    handle_step_failure(
                        step,
                        "timeout",
                        "step not executed due to deadline or abort",
                        ctx,
                        &mut errors,
                        &mut dummy_aborted,
                    );
                }
            }
        }
    }

    // Resolve response_merge template
    let mut body = resolve_template_value(&composition.response_merge, ctx);

    // Add _errors if any
    if !errors.is_empty() {
        if let Value::Object(ref mut map) = body {
            map.insert("_errors".to_string(), json!(errors));
        }
    }

    let status = if aborted { 502 } else { 200 };

    CompositionResult { body, status }
}

fn handle_step_failure(
    step: &CompositionStep,
    status: &str,
    message: &str,
    ctx: &mut TemplateContext,
    errors: &mut HashMap<String, StepError>,
    aborted: &mut bool,
) {
    match step.on_error.as_str() {
        "abort" => {
            errors.insert(
                step.name.clone(),
                StepError {
                    status: status.to_string(),
                    message: message.to_string(),
                },
            );
            *aborted = true;
        }
        "default" => {
            let default_val = step.default_value.clone().unwrap_or(Value::Null);
            ctx.steps.insert(
                step.name.clone(),
                StepResult {
                    status: 0,
                    headers: HashMap::new(),
                    body: default_val,
                },
            );
            errors.insert(
                step.name.clone(),
                StepError {
                    status: status.to_string(),
                    message: message.to_string(),
                },
            );
        }
        _ => {
            // "skip"
            ctx.steps.insert(
                step.name.clone(),
                StepResult {
                    status: 0,
                    headers: HashMap::new(),
                    body: Value::Null,
                },
            );
            errors.insert(
                step.name.clone(),
                StepError {
                    status: status.to_string(),
                    message: message.to_string(),
                },
            );
        }
    }
}

async fn execute_step(
    step: &CompositionStep,
    ctx: &TemplateContext,
    http_client: &reqwest::Client,
    target_resolver: &TargetResolver,
    timeout: Duration,
    internal_route_base: Option<&str>,
) -> Result<StepResult, String> {
    let path = resolve_template_string(&step.path_template, ctx);

    let url = if step.use_internal_route {
        // Route through the local proxy — enables SOAP translation, rate limits, etc.
        let base = internal_route_base
            .ok_or_else(|| "internal route requested but no proxy base URL configured".to_string())?;
        format!("{}{}", base.trim_end_matches('/'), path)
    } else {
        // Direct upstream call
        let targets = target_resolver(&step.upstream_id);
        if targets.is_empty() {
            return Err("no healthy targets for upstream".to_string());
        }
        let target = &targets[fastrand::usize(..targets.len())];
        let scheme = if target.tls { "https" } else { "http" };
        format!("{}://{}:{}{}", scheme, target.host, target.port, path)
    };

    let method = step
        .method
        .parse::<reqwest::Method>()
        .map_err(|e| format!("invalid method '{}': {}", step.method, e))?;

    let mut req = http_client.request(method, &url).timeout(timeout);

    // Apply headers template
    if let Some(ref headers_tmpl) = step.headers_template {
        let resolved = resolve_template_value(headers_tmpl, ctx);
        if let Value::Object(map) = resolved {
            for (k, v) in map {
                if let Value::String(val) = v {
                    req = req.header(&k, &val);
                }
            }
        }
    }

    // Apply body template
    if let Some(ref body_tmpl) = step.body_template {
        let resolved = resolve_template_value(body_tmpl, ctx);
        req = req.header("Content-Type", "application/json");
        req = req.json(&resolved);
    }

    let response = req
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    let status = response.status().as_u16();
    let mut headers = HashMap::new();
    for (k, v) in response.headers() {
        if let Ok(val) = v.to_str() {
            headers.insert(k.to_string(), val.to_string());
        }
    }

    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("failed to read response body: {}", e))?;

    let body: Value = serde_json::from_slice(&body_bytes).unwrap_or_else(|_| {
        Value::String(String::from_utf8_lossy(&body_bytes).to_string())
    });

    // Treat 5xx as errors
    if status >= 500 {
        return Err(format!("upstream returned status {}", status));
    }

    Ok(StepResult {
        status,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_valid_body() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name"]
        });
        let body = Some(json!({"name": "Alice", "age": 30}));
        assert!(validate_input_schema(&schema, &body).is_ok());
    }

    #[test]
    fn validate_missing_required() {
        let schema = json!({
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        });
        let body = Some(json!({"age": 30}));
        let err = validate_input_schema(&schema, &body).unwrap_err();
        assert!(err[0].contains("missing required field 'name'"));
    }

    #[test]
    fn validate_wrong_type() {
        let schema = json!({
            "type": "object",
            "properties": { "age": { "type": "integer" } }
        });
        let body = Some(json!({"age": "thirty"}));
        let err = validate_input_schema(&schema, &body).unwrap_err();
        assert!(err[0].contains("must be of type 'integer'"));
    }

    #[test]
    fn validate_no_body_with_required() {
        let schema = json!({
            "type": "object",
            "properties": { "x": { "type": "number" } },
            "required": ["x"]
        });
        let err = validate_input_schema(&schema, &None).unwrap_err();
        assert!(err[0].contains("request body is required"));
    }

    #[test]
    fn validate_no_body_no_required() {
        let schema = json!({
            "type": "object",
            "properties": { "x": { "type": "number" } }
        });
        assert!(validate_input_schema(&schema, &None).is_ok());
    }

    #[test]
    fn validate_extra_fields_ok() {
        let schema = json!({
            "type": "object",
            "properties": { "name": { "type": "string" } }
        });
        let body = Some(json!({"name": "Bob", "extra": true}));
        assert!(validate_input_schema(&schema, &body).is_ok());
    }
}
