use serde_json::Value;
use std::collections::HashMap;

/// Holds all data available for template resolution.
#[derive(Debug, Clone)]
pub struct TemplateContext {
    /// Named path params from the path_pattern (e.g. "id" from "/:id").
    pub path_params: HashMap<String, String>,
    /// Query parameters from the incoming request.
    pub query_params: HashMap<String, String>,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// Parsed JSON request body.
    pub body: Option<Value>,
    /// Completed step results keyed by step name.
    pub steps: HashMap<String, StepResult>,
}

/// Result of a completed composition step.
#[derive(Debug, Clone)]
pub struct StepResult {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Value,
}

impl TemplateContext {
    pub fn new() -> Self {
        Self {
            path_params: HashMap::new(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            steps: HashMap::new(),
        }
    }
}

/// Extract named path params from a path_pattern and actual path.
/// e.g. pattern="/:id/orders" path="/123/orders" -> {"id": "123"}
pub fn extract_path_params(pattern: &str, path: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    let pattern_parts: Vec<&str> = pattern.split('/').collect();
    let path_parts: Vec<&str> = path.split('/').collect();

    for (pp, actual) in pattern_parts.iter().zip(path_parts.iter()) {
        if let Some(name) = pp.strip_prefix(':') {
            params.insert(name.to_string(), actual.to_string());
        }
    }
    params
}

/// Resolve a template string by replacing all `${...}` placeholders with values from context.
/// Returns the resolved string.
pub fn resolve_template_string(template: &str, ctx: &TemplateContext) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut expr = String::new();
            let mut depth = 1;
            for c in chars.by_ref() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                expr.push(c);
            }
            let resolved = resolve_expression(&expr, ctx);
            result.push_str(&resolved);
        } else {
            result.push(c);
        }
    }

    result
}

/// Resolve a JSON value template — handles string interpolation and whole-value substitution.
/// If a JSON string value is exactly `${expr}`, it's replaced with the resolved value (preserving type).
/// If it contains `${expr}` among other text, string interpolation is applied.
pub fn resolve_template_value(template: &Value, ctx: &TemplateContext) -> Value {
    match template {
        Value::String(s) => {
            // Check for exact whole-value substitution: "${expr}"
            let trimmed = s.trim();
            if trimmed.starts_with("${") && trimmed.ends_with('}') && count_expressions(trimmed) == 1 {
                let expr = &trimmed[2..trimmed.len() - 1];
                let (expr_part, default) = split_default(expr);
                let val = resolve_expression_to_value(expr_part, ctx);
                if val.is_null() {
                    if let Some(d) = default {
                        Value::String(d)
                    } else {
                        val
                    }
                } else {
                    val
                }
            } else if s.contains("${") {
                Value::String(resolve_template_string(s, ctx))
            } else {
                Value::String(s.clone())
            }
        }
        Value::Object(map) => {
            let mut result = serde_json::Map::new();
            for (k, v) in map {
                result.insert(k.clone(), resolve_template_value(v, ctx));
            }
            Value::Object(result)
        }
        Value::Array(arr) => {
            Value::Array(arr.iter().map(|v| resolve_template_value(v, ctx)).collect())
        }
        other => other.clone(),
    }
}

/// Count how many `${...}` expressions are in a string.
fn count_expressions(s: &str) -> usize {
    let mut count = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut depth = 1;
            for c in chars.by_ref() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            count += 1;
        }
    }
    count
}

/// Resolve an expression like "request.path.id" or "user.body.email" to a string.
/// Supports default values with pipe syntax: `${request.query.limit|10}`
fn resolve_expression(expr: &str, ctx: &TemplateContext) -> String {
    let (expr_part, default) = split_default(expr);
    let val = resolve_expression_to_value(expr_part, ctx);
    match val {
        Value::String(s) => s,
        Value::Null => default.unwrap_or_default(),
        other => other.to_string(),
    }
}

/// Split an expression into the main part and an optional default value.
/// e.g. "request.query.limit|10" -> ("request.query.limit", Some("10"))
fn split_default(expr: &str) -> (&str, Option<String>) {
    if let Some(idx) = expr.rfind('|') {
        let main = &expr[..idx];
        let default = &expr[idx + 1..];
        // Only treat as default if the main part looks like a valid expression
        if !main.is_empty() && !default.is_empty() {
            return (main, Some(default.to_string()));
        }
    }
    (expr, None)
}

/// Resolve an expression to a serde_json::Value, preserving types.
fn resolve_expression_to_value(expr: &str, ctx: &TemplateContext) -> Value {
    let parts: Vec<&str> = expr.splitn(3, '.').collect();
    if parts.is_empty() {
        return Value::Null;
    }

    if parts[0] == "request" {
        if parts.len() < 2 {
            return Value::Null;
        }
        match parts[1] {
            "path" => {
                if parts.len() < 3 {
                    return Value::Null;
                }
                ctx.path_params
                    .get(parts[2])
                    .map(|v| Value::String(v.clone()))
                    .unwrap_or(Value::Null)
            }
            "query" => {
                if parts.len() < 3 {
                    return Value::Null;
                }
                ctx.query_params
                    .get(parts[2])
                    .map(|v| Value::String(v.clone()))
                    .unwrap_or(Value::Null)
            }
            "header" => {
                if parts.len() < 3 {
                    return Value::Null;
                }
                ctx.headers
                    .get(parts[2])
                    .map(|v| Value::String(v.clone()))
                    .unwrap_or(Value::Null)
            }
            "body" => {
                if parts.len() < 3 {
                    return ctx.body.clone().unwrap_or(Value::Null);
                }
                navigate_json(
                    ctx.body.as_ref().unwrap_or(&Value::Null),
                    parts[2],
                )
            }
            _ => Value::Null,
        }
    } else {
        // Step reference: stepName.body.field or stepName.status or stepName.header.X-Foo
        let step_name = parts[0];
        let step = match ctx.steps.get(step_name) {
            Some(s) => s,
            None => return Value::Null,
        };

        if parts.len() < 2 {
            return Value::Null;
        }

        match parts[1] {
            "body" => {
                if parts.len() < 3 {
                    return step.body.clone();
                }
                navigate_json(&step.body, parts[2])
            }
            "status" => Value::Number(step.status.into()),
            "header" => {
                if parts.len() < 3 {
                    return Value::Null;
                }
                step.headers
                    .get(parts[2])
                    .map(|v| Value::String(v.clone()))
                    .unwrap_or(Value::Null)
            }
            _ => Value::Null,
        }
    }
}

/// Navigate a JSON value by a dot-separated path.
fn navigate_json(value: &Value, path: &str) -> Value {
    let mut current = value;
    for part in path.split('.') {
        match current {
            Value::Object(map) => {
                current = match map.get(part) {
                    Some(v) => v,
                    None => return Value::Null,
                };
            }
            Value::Array(arr) => {
                if let Ok(idx) = part.parse::<usize>() {
                    current = match arr.get(idx) {
                        Some(v) => v,
                        None => return Value::Null,
                    };
                } else {
                    return Value::Null;
                }
            }
            _ => return Value::Null,
        }
    }
    current.clone()
}

/// Validate that all template expressions in a value use valid syntax.
/// Returns a list of invalid expressions found.
pub fn validate_template(value: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    validate_template_inner(value, &mut errors);
    errors
}

fn validate_template_inner(value: &Value, errors: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            let expressions = extract_expressions(s);
            for raw_expr in expressions {
                // Strip optional default value before validating
                let (expr, _) = split_default(&raw_expr);
                let parts: Vec<&str> = expr.splitn(3, '.').collect();
                if parts.is_empty() || parts[0].is_empty() {
                    errors.push(format!("empty expression in '{}'", s));
                    continue;
                }
                if parts[0] == "request" {
                    if parts.len() < 2 {
                        errors.push(format!("incomplete request expression: '{}'", expr));
                    } else if !["path", "query", "header", "body"].contains(&parts[1]) {
                        errors.push(format!("unknown request accessor '{}' in '{}'", parts[1], expr));
                    }
                }
                // Step references are valid as long as they have at least stepName.accessor
                else if parts.len() < 2 {
                    errors.push(format!("incomplete step expression: '{}'", expr));
                } else if !["body", "status", "header"].contains(&parts[1]) {
                    errors.push(format!("unknown step accessor '{}' in '{}'", parts[1], expr));
                }
            }
        }
        Value::Object(map) => {
            for v in map.values() {
                validate_template_inner(v, errors);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                validate_template_inner(v, errors);
            }
        }
        _ => {}
    }
}

/// Extract all expression strings from `${...}` placeholders in a string.
fn extract_expressions(s: &str) -> Vec<String> {
    let mut exprs = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next();
            let mut expr = String::new();
            let mut depth = 1;
            for c in chars.by_ref() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                expr.push(c);
            }
            exprs.push(expr);
        }
    }
    exprs
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_ctx() -> TemplateContext {
        let mut ctx = TemplateContext::new();
        ctx.path_params.insert("id".to_string(), "42".to_string());
        ctx.query_params.insert("page".to_string(), "1".to_string());
        ctx.headers.insert("X-Foo".to_string(), "bar".to_string());
        ctx.body = Some(json!({"email": "test@example.com", "nested": {"key": "val"}}));
        ctx.steps.insert("user".to_string(), StepResult {
            status: 200,
            headers: {
                let mut h = HashMap::new();
                h.insert("Content-Type".to_string(), "application/json".to_string());
                h
            },
            body: json!({"name": "John", "email": "john@example.com", "address": {"city": "NYC"}}),
        });
        ctx
    }

    #[test]
    fn resolve_request_path_param() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("/users/${request.path.id}", &ctx), "/users/42");
    }

    #[test]
    fn resolve_request_query_param() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("?page=${request.query.page}", &ctx), "?page=1");
    }

    #[test]
    fn resolve_request_header() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("${request.header.X-Foo}", &ctx), "bar");
    }

    #[test]
    fn resolve_request_body_field() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("${request.body.email}", &ctx), "test@example.com");
    }

    #[test]
    fn resolve_request_body_nested() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("${request.body.nested.key}", &ctx), "val");
    }

    #[test]
    fn resolve_step_body() {
        let ctx = make_ctx();
        let val = resolve_template_value(&json!("${user.body}"), &ctx);
        assert_eq!(val, json!({"name": "John", "email": "john@example.com", "address": {"city": "NYC"}}));
    }

    #[test]
    fn resolve_step_body_field() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("${user.body.name}", &ctx), "John");
    }

    #[test]
    fn resolve_step_body_nested() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("${user.body.address.city}", &ctx), "NYC");
    }

    #[test]
    fn resolve_step_status() {
        let ctx = make_ctx();
        let val = resolve_template_value(&json!("${user.status}"), &ctx);
        assert_eq!(val, json!(200));
    }

    #[test]
    fn resolve_step_header() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("${user.header.Content-Type}", &ctx), "application/json");
    }

    #[test]
    fn resolve_missing_returns_empty() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("${request.path.missing}", &ctx), "");
        assert_eq!(resolve_template_string("${nonexistent.body}", &ctx), "");
    }

    #[test]
    fn resolve_template_value_preserves_types() {
        let ctx = make_ctx();
        let template = json!({
            "user": "${user.body}",
            "id": "${request.path.id}",
            "static": "hello"
        });
        let result = resolve_template_value(&template, &ctx);
        assert_eq!(result["user"]["name"], "John");
        assert_eq!(result["id"], "42"); // path params are strings
        assert_eq!(result["static"], "hello");
    }

    #[test]
    fn resolve_string_interpolation() {
        let ctx = make_ctx();
        assert_eq!(
            resolve_template_string("/users/${request.path.id}/orders", &ctx),
            "/users/42/orders"
        );
    }

    #[test]
    fn no_template_passthrough() {
        let ctx = make_ctx();
        assert_eq!(resolve_template_string("plain string", &ctx), "plain string");
    }

    #[test]
    fn extract_path_params_basic() {
        let params = extract_path_params("/:id", "/42");
        assert_eq!(params.get("id"), Some(&"42".to_string()));
    }

    #[test]
    fn extract_path_params_multiple() {
        let params = extract_path_params("/:userId/orders/:orderId", "/5/orders/99");
        assert_eq!(params.get("userId"), Some(&"5".to_string()));
        assert_eq!(params.get("orderId"), Some(&"99".to_string()));
    }

    #[test]
    fn validate_valid_template() {
        let template = json!({
            "user": "${user.body}",
            "path": "${request.path.id}"
        });
        let errors = validate_template(&template);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }

    #[test]
    fn validate_invalid_accessor() {
        let template = json!("${request.invalid.foo}");
        let errors = validate_template(&template);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("unknown request accessor"));
    }

    #[test]
    fn validate_incomplete_step() {
        let template = json!("${user}");
        let errors = validate_template(&template);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("incomplete step expression"));
    }

    #[test]
    fn body_template_resolution() {
        let ctx = make_ctx();
        let body_template = json!({"email": "${user.body.email}"});
        let resolved = resolve_template_value(&body_template, &ctx);
        assert_eq!(resolved, json!({"email": "john@example.com"}));
    }

    #[test]
    fn resolve_default_value_when_missing() {
        let ctx = make_ctx();
        // Missing query param with default
        assert_eq!(resolve_template_string("?limit=${request.query.limit|10}", &ctx), "?limit=10");
        // Existing query param ignores default
        assert_eq!(resolve_template_string("?page=${request.query.page|5}", &ctx), "?page=1");
    }

    #[test]
    fn resolve_default_value_whole_expression() {
        let ctx = make_ctx();
        let val = resolve_template_value(&json!("${request.query.missing|fallback}"), &ctx);
        assert_eq!(val, json!("fallback"));
    }

    #[test]
    fn validate_template_with_default() {
        let template = json!("${request.query.limit|10}");
        let errors = validate_template(&template);
        assert!(errors.is_empty(), "unexpected errors: {:?}", errors);
    }
}
