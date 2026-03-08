use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Deserializer, Serialize};

fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}
use sqlx::PgPool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::routes::upstreams::PaginationParams;

// --- DTOs ---

#[derive(Deserialize)]
pub struct CreateComposition {
    pub name: String,
    pub path_prefix: String,
    pub path_pattern: Option<String>,
    pub methods: Option<Vec<String>>,
    pub host_pattern: Option<String>,
    pub timeout_ms: Option<i32>,
    pub max_wait_ms: Option<i32>,
    pub auth_skip: Option<bool>,
    pub response_merge: serde_json::Value,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
    pub namespace: Option<String>,
    pub steps: Vec<CreateStep>,
}

#[derive(Deserialize)]
pub struct CreateStep {
    pub name: String,
    pub step_order: Option<i32>,
    pub method: Option<String>,
    pub upstream_id: Uuid,
    pub path_template: String,
    pub body_template: Option<serde_json::Value>,
    pub headers_template: Option<serde_json::Value>,
    pub depends_on: Option<Vec<String>>,
    pub on_error: Option<String>,
    pub default_value: Option<serde_json::Value>,
    pub timeout_ms: Option<i32>,
    pub use_internal_route: Option<bool>,
}

#[derive(Deserialize)]
pub struct UpdateComposition {
    pub name: Option<String>,
    pub path_prefix: Option<String>,
    pub path_pattern: Option<Option<String>>,
    pub methods: Option<Vec<String>>,
    pub host_pattern: Option<Option<String>>,
    pub timeout_ms: Option<i32>,
    pub max_wait_ms: Option<Option<i32>>,
    pub auth_skip: Option<bool>,
    pub active: Option<bool>,
    pub response_merge: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub input_schema: Option<Option<serde_json::Value>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub output_schema: Option<Option<serde_json::Value>>,
    #[serde(default, deserialize_with = "deserialize_some")]
    pub namespace: Option<Option<String>>,
    pub steps: Option<Vec<CreateStep>>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct CompositionResponse {
    pub id: Uuid,
    pub name: String,
    pub path_prefix: String,
    pub path_pattern: Option<String>,
    pub methods: Option<Vec<String>>,
    pub host_pattern: Option<String>,
    pub timeout_ms: i32,
    pub max_wait_ms: Option<i32>,
    pub auth_skip: bool,
    pub active: bool,
    pub response_merge: serde_json::Value,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
    pub namespace: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct CompositionWithSteps {
    #[serde(flatten)]
    pub composition: CompositionResponse,
    pub steps: Vec<StepResponse>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct StepResponse {
    pub id: Uuid,
    pub composition_id: Uuid,
    pub name: String,
    pub step_order: i32,
    pub method: String,
    pub upstream_id: Uuid,
    pub path_template: String,
    pub body_template: Option<serde_json::Value>,
    pub headers_template: Option<serde_json::Value>,
    pub depends_on: Option<Vec<String>>,
    pub on_error: String,
    pub default_value: Option<serde_json::Value>,
    pub timeout_ms: i32,
    pub use_internal_route: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Serialize)]
pub struct ListResponse<T> {
    pub data: Vec<T>,
    pub total: i64,
    pub page: i64,
    pub limit: i64,
}

// --- Handlers ---

pub async fn list_compositions(
    State(pool): State<PgPool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<CompositionResponse>>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(20).clamp(1, 100);
    let offset = (page - 1) * limit;

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM compositions")
        .fetch_one(&pool)
        .await?;

    let rows: Vec<CompositionResponse> = sqlx::query_as(
        "SELECT * FROM compositions ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    Ok(Json(ListResponse {
        data: rows,
        total: total.0,
        page,
        limit,
    }))
}

pub async fn create_composition(
    State(pool): State<PgPool>,
    Json(body): Json<CreateComposition>,
) -> Result<(axum::http::StatusCode, Json<CompositionWithSteps>), AppError> {
    if body.name.trim().is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if !body.path_prefix.starts_with('/') {
        return Err(AppError::Validation("path_prefix must start with '/'".into()));
    }
    if body.steps.is_empty() {
        return Err(AppError::Validation("at least one step is required".into()));
    }

    // Validate template expressions
    let template_errors = shared::composition::template::validate_template(&body.response_merge);
    if !template_errors.is_empty() {
        return Err(AppError::Validation(format!(
            "invalid response_merge template: {}",
            template_errors.join("; ")
        )));
    }

    // Validate on_error values and build DAG for cycle detection
    let mut dag_steps = Vec::new();
    for (i, step) in body.steps.iter().enumerate() {
        if step.name.trim().is_empty() {
            return Err(AppError::Validation(format!("step {} name is required", i)));
        }
        let on_error = step.on_error.as_deref().unwrap_or("abort");
        if !["abort", "skip", "default"].contains(&on_error) {
            return Err(AppError::Validation(format!(
                "step '{}' on_error must be 'abort', 'skip', or 'default'",
                step.name
            )));
        }
        dag_steps.push((
            step.name.clone(),
            step.depends_on.clone().unwrap_or_default(),
        ));
    }

    shared::composition::dag::validate_dag(&dag_steps)
        .map_err(|e| AppError::Validation(format!("invalid step dependencies: {}", e)))?;

    let timeout_ms = body.timeout_ms.unwrap_or(30000);
    let auth_skip = body.auth_skip.unwrap_or(false);

    let comp: CompositionResponse = sqlx::query_as(
        "INSERT INTO compositions (name, path_prefix, path_pattern, methods, host_pattern, timeout_ms, max_wait_ms, auth_skip, response_merge, input_schema, output_schema, namespace) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) RETURNING *",
    )
    .bind(body.name.trim())
    .bind(&body.path_prefix)
    .bind(&body.path_pattern)
    .bind(&body.methods)
    .bind(&body.host_pattern)
    .bind(timeout_ms)
    .bind(body.max_wait_ms)
    .bind(auth_skip)
    .bind(&body.response_merge)
    .bind(&body.input_schema)
    .bind(&body.output_schema)
    .bind(&body.namespace)
    .fetch_one(&pool)
    .await?;

    let mut step_responses = Vec::new();
    for (i, step) in body.steps.iter().enumerate() {
        let step_row: StepResponse = sqlx::query_as(
            "INSERT INTO composition_steps (composition_id, name, step_order, method, upstream_id, path_template, body_template, headers_template, depends_on, on_error, default_value, timeout_ms, use_internal_route) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) RETURNING *",
        )
        .bind(comp.id)
        .bind(step.name.trim())
        .bind(step.step_order.unwrap_or(i as i32))
        .bind(step.method.as_deref().unwrap_or("GET"))
        .bind(step.upstream_id)
        .bind(&step.path_template)
        .bind(&step.body_template)
        .bind(&step.headers_template)
        .bind(&step.depends_on)
        .bind(step.on_error.as_deref().unwrap_or("abort"))
        .bind(&step.default_value)
        .bind(step.timeout_ms.unwrap_or(10000))
        .bind(step.use_internal_route.unwrap_or(false))
        .fetch_one(&pool)
        .await?;
        step_responses.push(step_row);
    }

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CompositionWithSteps {
            composition: comp,
            steps: step_responses,
        }),
    ))
}

pub async fn get_composition(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<CompositionWithSteps>, AppError> {
    let comp: CompositionResponse =
        sqlx::query_as("SELECT * FROM compositions WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Composition not found".into()))?;

    let steps: Vec<StepResponse> = sqlx::query_as(
        "SELECT * FROM composition_steps WHERE composition_id = $1 ORDER BY step_order",
    )
    .bind(id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(CompositionWithSteps {
        composition: comp,
        steps,
    }))
}

pub async fn update_composition(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateComposition>,
) -> Result<Json<CompositionWithSteps>, AppError> {
    let existing: CompositionResponse =
        sqlx::query_as("SELECT * FROM compositions WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Composition not found".into()))?;

    let name = body.name.unwrap_or(existing.name);
    let path_prefix = body.path_prefix.unwrap_or(existing.path_prefix);
    let path_pattern = if let Some(pp) = body.path_pattern { pp } else { existing.path_pattern };
    let methods = if body.methods.is_some() { body.methods } else { existing.methods };
    let host_pattern = if let Some(hp) = body.host_pattern { hp } else { existing.host_pattern };
    let timeout_ms = body.timeout_ms.unwrap_or(existing.timeout_ms);
    let max_wait_ms = if let Some(mw) = body.max_wait_ms { mw } else { existing.max_wait_ms };
    let auth_skip = body.auth_skip.unwrap_or(existing.auth_skip);
    let active = body.active.unwrap_or(existing.active);
    let response_merge = body.response_merge.unwrap_or(existing.response_merge);
    let input_schema = match body.input_schema {
        Some(None) => None,                  // explicit null → clear
        Some(Some(v)) => Some(v),
        None => existing.input_schema,       // absent → keep
    };
    let output_schema = match body.output_schema {
        Some(None) => None,
        Some(Some(v)) => Some(v),
        None => existing.output_schema,
    };
    let namespace = match body.namespace {
        Some(None) => None,
        Some(Some(v)) => Some(v),
        None => existing.namespace,
    };

    if !path_prefix.starts_with('/') {
        return Err(AppError::Validation("path_prefix must start with '/'".into()));
    }

    let updated: CompositionResponse = sqlx::query_as(
        "UPDATE compositions SET name = $1, path_prefix = $2, path_pattern = $3, methods = $4, host_pattern = $5, timeout_ms = $6, max_wait_ms = $7, auth_skip = $8, active = $9, response_merge = $10, input_schema = $11, output_schema = $12, namespace = $13, updated_at = now() WHERE id = $14 RETURNING *",
    )
    .bind(&name)
    .bind(&path_prefix)
    .bind(&path_pattern)
    .bind(&methods)
    .bind(&host_pattern)
    .bind(timeout_ms)
    .bind(max_wait_ms)
    .bind(auth_skip)
    .bind(active)
    .bind(&response_merge)
    .bind(&input_schema)
    .bind(&output_schema)
    .bind(&namespace)
    .bind(id)
    .fetch_one(&pool)
    .await?;

    // If steps are provided, replace all steps
    if let Some(new_steps) = body.steps {
        // Validate DAG
        let mut dag_steps = Vec::new();
        for (i, step) in new_steps.iter().enumerate() {
            if step.name.trim().is_empty() {
                return Err(AppError::Validation(format!("step {} name is required", i)));
            }
            let on_error = step.on_error.as_deref().unwrap_or("abort");
            if !["abort", "skip", "default"].contains(&on_error) {
                return Err(AppError::Validation(format!(
                    "step '{}' on_error must be 'abort', 'skip', or 'default'",
                    step.name
                )));
            }
            dag_steps.push((
                step.name.clone(),
                step.depends_on.clone().unwrap_or_default(),
            ));
        }
        shared::composition::dag::validate_dag(&dag_steps)
            .map_err(|e| AppError::Validation(format!("invalid step dependencies: {}", e)))?;

        // Delete existing steps and insert new ones
        sqlx::query("DELETE FROM composition_steps WHERE composition_id = $1")
            .bind(id)
            .execute(&pool)
            .await?;

        for (i, step) in new_steps.iter().enumerate() {
            sqlx::query(
                "INSERT INTO composition_steps (composition_id, name, step_order, method, upstream_id, path_template, body_template, headers_template, depends_on, on_error, default_value, timeout_ms, use_internal_route) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            )
            .bind(id)
            .bind(step.name.trim())
            .bind(step.step_order.unwrap_or(i as i32))
            .bind(step.method.as_deref().unwrap_or("GET"))
            .bind(step.upstream_id)
            .bind(&step.path_template)
            .bind(&step.body_template)
            .bind(&step.headers_template)
            .bind(&step.depends_on)
            .bind(step.on_error.as_deref().unwrap_or("abort"))
            .bind(&step.default_value)
            .bind(step.timeout_ms.unwrap_or(10000))
            .bind(step.use_internal_route.unwrap_or(false))
            .execute(&pool)
            .await?;
        }
    }

    let steps: Vec<StepResponse> = sqlx::query_as(
        "SELECT * FROM composition_steps WHERE composition_id = $1 ORDER BY step_order",
    )
    .bind(id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(CompositionWithSteps {
        composition: updated,
        steps,
    }))
}

pub async fn get_composition_openapi(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let comp: CompositionResponse =
        sqlx::query_as("SELECT * FROM compositions WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Composition not found".into()))?;

    let spec = build_openapi_spec(&comp);
    Ok(Json(spec))
}

fn build_openapi_spec(comp: &CompositionResponse) -> serde_json::Value {
    let method = comp
        .methods
        .as_ref()
        .and_then(|m| m.first())
        .map(|m| m.to_lowercase())
        .unwrap_or_else(|| "post".to_string());

    let path_key = format!(
        "/gateway{}{}",
        comp.path_prefix,
        comp.path_pattern.as_deref().unwrap_or("")
    );

    let mut operation = serde_json::json!({
        "summary": comp.name,
        "operationId": comp.name.replace(' ', "_"),
    });

    if let Some(ref schema) = comp.input_schema {
        operation["requestBody"] = serde_json::json!({
            "required": true,
            "content": {
                "application/json": {
                    "schema": schema
                }
            }
        });
    }

    let response_schema = comp
        .output_schema
        .as_ref()
        .cloned()
        .unwrap_or_else(|| serde_json::json!({"type": "object"}));

    operation["responses"] = serde_json::json!({
        "200": {
            "description": "Successful response",
            "content": {
                "application/json": {
                    "schema": response_schema
                }
            }
        }
    });

    serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": comp.name,
            "version": "1.0.0"
        },
        "paths": {
            path_key: {
                method: operation
            }
        }
    })
}

#[derive(Serialize)]
pub struct NamespaceSummary {
    pub namespace: Option<String>,
    pub count: i64,
}

pub async fn list_namespaces(
    State(pool): State<PgPool>,
) -> Result<Json<Vec<NamespaceSummary>>, AppError> {
    let rows: Vec<(Option<String>, i64)> = sqlx::query_as(
        "SELECT namespace, COUNT(*) as count FROM compositions GROUP BY namespace ORDER BY namespace NULLS LAST",
    )
    .fetch_all(&pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|(namespace, count)| NamespaceSummary { namespace, count })
            .collect(),
    ))
}

pub async fn get_namespace_openapi(
    State(pool): State<PgPool>,
    Path(ns): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows: Vec<CompositionResponse> = if ns == "_ungrouped" {
        sqlx::query_as("SELECT * FROM compositions WHERE namespace IS NULL ORDER BY created_at")
            .fetch_all(&pool)
            .await?
    } else {
        sqlx::query_as("SELECT * FROM compositions WHERE namespace = $1 ORDER BY created_at")
            .bind(&ns)
            .fetch_all(&pool)
            .await?
    };

    if rows.is_empty() {
        return Err(AppError::NotFound("Namespace not found or empty".into()));
    }

    let spec = build_combined_openapi_spec(&ns, &rows);
    Ok(Json(spec))
}

fn build_combined_openapi_spec(ns: &str, compositions: &[CompositionResponse]) -> serde_json::Value {
    let title = if ns == "_ungrouped" { "Ungrouped Compositions" } else { ns };
    let mut paths = serde_json::Map::new();

    for comp in compositions {
        let single_spec = build_openapi_spec(comp);
        if let Some(spec_paths) = single_spec.get("paths").and_then(|p| p.as_object()) {
            for (path, methods) in spec_paths {
                paths.insert(path.clone(), methods.clone());
            }
        }
    }

    serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": title,
            "version": "1.0.0"
        },
        "paths": paths
    })
}

pub async fn delete_composition(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM compositions WHERE id = $1")
        .bind(id)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Composition not found".into()));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
