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
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::errors::AppError;
use crate::routes::PaginationParams;

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
pub struct CompositionRow {
    pub id: String,
    pub name: String,
    pub path_prefix: String,
    pub path_pattern: Option<String>,
    pub methods: Option<String>,
    pub host_pattern: Option<String>,
    pub timeout_ms: i32,
    pub max_wait_ms: Option<i32>,
    pub auth_skip: bool,
    pub active: bool,
    pub response_merge: String,
    pub input_schema: Option<String>,
    pub output_schema: Option<String>,
    pub namespace: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct CompositionResponse {
    pub id: String,
    pub name: String,
    pub path_prefix: String,
    pub path_pattern: Option<String>,
    pub methods: Option<serde_json::Value>,
    pub host_pattern: Option<String>,
    pub timeout_ms: i32,
    pub max_wait_ms: Option<i32>,
    pub auth_skip: bool,
    pub active: bool,
    pub response_merge: serde_json::Value,
    pub input_schema: Option<serde_json::Value>,
    pub output_schema: Option<serde_json::Value>,
    pub namespace: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<CompositionRow> for CompositionResponse {
    fn from(r: CompositionRow) -> Self {
        let methods: Option<serde_json::Value> = r
            .methods
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let response_merge: serde_json::Value =
            serde_json::from_str(&r.response_merge).unwrap_or_default();
        let input_schema: Option<serde_json::Value> = r
            .input_schema
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let output_schema: Option<serde_json::Value> = r
            .output_schema
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        Self {
            id: r.id,
            name: r.name,
            path_prefix: r.path_prefix,
            path_pattern: r.path_pattern,
            methods,
            host_pattern: r.host_pattern,
            timeout_ms: r.timeout_ms,
            max_wait_ms: r.max_wait_ms,
            auth_skip: r.auth_skip,
            active: r.active,
            response_merge,
            input_schema,
            output_schema,
            namespace: r.namespace,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Serialize, sqlx::FromRow)]
pub struct StepRow {
    pub id: String,
    pub composition_id: String,
    pub name: String,
    pub step_order: i32,
    pub method: String,
    pub upstream_id: String,
    pub path_template: String,
    pub body_template: Option<String>,
    pub headers_template: Option<String>,
    pub depends_on: Option<String>,
    pub on_error: String,
    pub default_value: Option<String>,
    pub timeout_ms: i32,
    pub use_internal_route: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
pub struct StepResponse {
    pub id: String,
    pub composition_id: String,
    pub name: String,
    pub step_order: i32,
    pub method: String,
    pub upstream_id: String,
    pub path_template: String,
    pub body_template: Option<serde_json::Value>,
    pub headers_template: Option<serde_json::Value>,
    pub depends_on: Option<serde_json::Value>,
    pub on_error: String,
    pub default_value: Option<serde_json::Value>,
    pub timeout_ms: i32,
    pub use_internal_route: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<StepRow> for StepResponse {
    fn from(s: StepRow) -> Self {
        Self {
            id: s.id,
            composition_id: s.composition_id,
            name: s.name,
            step_order: s.step_order,
            method: s.method,
            upstream_id: s.upstream_id,
            path_template: s.path_template,
            body_template: s.body_template.and_then(|v| serde_json::from_str(&v).ok()),
            headers_template: s.headers_template.and_then(|v| serde_json::from_str(&v).ok()),
            depends_on: s.depends_on.and_then(|v| serde_json::from_str(&v).ok()),
            on_error: s.on_error,
            default_value: s.default_value.and_then(|v| serde_json::from_str(&v).ok()),
            timeout_ms: s.timeout_ms,
            use_internal_route: s.use_internal_route,
            created_at: s.created_at,
            updated_at: s.updated_at,
        }
    }
}

#[derive(Serialize)]
pub struct CompositionWithSteps {
    #[serde(flatten)]
    pub composition: CompositionResponse,
    pub steps: Vec<StepResponse>,
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
    State(pool): State<SqlitePool>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<ListResponse<CompositionResponse>>, AppError> {
    let (page, limit, offset) = params.resolve();

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM compositions")
        .fetch_one(&pool)
        .await?;

    let rows: Vec<CompositionRow> = sqlx::query_as(
        "SELECT * FROM compositions ORDER BY created_at DESC LIMIT ?1 OFFSET ?2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    Ok(Json(ListResponse {
        data: rows.into_iter().map(CompositionResponse::from).collect(),
        total: total.0,
        page,
        limit,
    }))
}

pub async fn create_composition(
    State(pool): State<SqlitePool>,
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

    let template_errors = shared::composition::template::validate_template(&body.response_merge);
    if !template_errors.is_empty() {
        return Err(AppError::Validation(format!(
            "invalid response_merge template: {}",
            template_errors.join("; ")
        )));
    }

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
    let methods_json = body.methods.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default());
    let response_merge_json = serde_json::to_string(&body.response_merge).unwrap_or_else(|_| "{}".to_string());
    let input_schema_json = body.input_schema.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
    let output_schema_json = body.output_schema.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

    let comp_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO compositions (id, name, path_prefix, path_pattern, methods, host_pattern, timeout_ms, max_wait_ms, auth_skip, response_merge, input_schema, output_schema, namespace) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
    )
    .bind(&comp_id)
    .bind(body.name.trim())
    .bind(&body.path_prefix)
    .bind(&body.path_pattern)
    .bind(&methods_json)
    .bind(&body.host_pattern)
    .bind(timeout_ms)
    .bind(body.max_wait_ms)
    .bind(auth_skip)
    .bind(&response_merge_json)
    .bind(&input_schema_json)
    .bind(&output_schema_json)
    .bind(&body.namespace)
    .execute(&pool)
    .await?;

    let comp_row: CompositionRow = sqlx::query_as("SELECT * FROM compositions WHERE id = ?1")
        .bind(&comp_id)
        .fetch_one(&pool)
        .await?;

    let mut step_responses = Vec::new();
    for (i, step) in body.steps.iter().enumerate() {
        let step_id = Uuid::new_v4().to_string();
        let body_tmpl = step.body_template.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
        let headers_tmpl = step.headers_template.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
        let depends_on_json = step.depends_on.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
        let default_val = step.default_value.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

        sqlx::query(
            "INSERT INTO composition_steps (id, composition_id, name, step_order, method, upstream_id, path_template, body_template, headers_template, depends_on, on_error, default_value, timeout_ms, use_internal_route) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .bind(&step_id)
        .bind(&comp_id)
        .bind(step.name.trim())
        .bind(step.step_order.unwrap_or(i as i32))
        .bind(step.method.as_deref().unwrap_or("GET"))
        .bind(step.upstream_id.to_string())
        .bind(&step.path_template)
        .bind(&body_tmpl)
        .bind(&headers_tmpl)
        .bind(&depends_on_json)
        .bind(step.on_error.as_deref().unwrap_or("abort"))
        .bind(&default_val)
        .bind(step.timeout_ms.unwrap_or(10000))
        .bind(step.use_internal_route.unwrap_or(false))
        .execute(&pool)
        .await?;

        let step_row: StepRow = sqlx::query_as("SELECT * FROM composition_steps WHERE id = ?1")
            .bind(&step_id)
            .fetch_one(&pool)
            .await?;
        step_responses.push(StepResponse::from(step_row));
    }

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CompositionWithSteps {
            composition: CompositionResponse::from(comp_row),
            steps: step_responses,
        }),
    ))
}

pub async fn get_composition(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<Json<CompositionWithSteps>, AppError> {
    let comp_row: CompositionRow = sqlx::query_as("SELECT * FROM compositions WHERE id = ?1")
        .bind(id.to_string())
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Composition not found".into()))?;

    let step_rows: Vec<StepRow> = sqlx::query_as(
        "SELECT * FROM composition_steps WHERE composition_id = ?1 ORDER BY step_order",
    )
    .bind(id.to_string())
    .fetch_all(&pool)
    .await?;

    Ok(Json(CompositionWithSteps {
        composition: CompositionResponse::from(comp_row),
        steps: step_rows.into_iter().map(StepResponse::from).collect(),
    }))
}

pub async fn update_composition(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateComposition>,
) -> Result<Json<CompositionWithSteps>, AppError> {
    let existing: CompositionRow = sqlx::query_as("SELECT * FROM compositions WHERE id = ?1")
        .bind(id.to_string())
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Composition not found".into()))?;

    let name = body.name.unwrap_or(existing.name);
    let path_prefix = body.path_prefix.unwrap_or(existing.path_prefix);
    let path_pattern = if let Some(pp) = body.path_pattern { pp } else { existing.path_pattern };
    let methods_json = if let Some(ref methods) = body.methods {
        Some(serde_json::to_string(methods).unwrap_or_default())
    } else {
        existing.methods
    };
    let host_pattern = if let Some(hp) = body.host_pattern { hp } else { existing.host_pattern };
    let timeout_ms = body.timeout_ms.unwrap_or(existing.timeout_ms);
    let max_wait_ms = if let Some(mw) = body.max_wait_ms { mw } else { existing.max_wait_ms };
    let auth_skip = body.auth_skip.unwrap_or(existing.auth_skip);
    let active = body.active.unwrap_or(existing.active);
    let response_merge_json = if let Some(ref rm) = body.response_merge {
        serde_json::to_string(rm).unwrap_or_else(|_| "{}".to_string())
    } else {
        existing.response_merge
    };
    let input_schema_json = match body.input_schema {
        Some(None) => None,                  // explicit null → clear
        Some(Some(ref v)) => Some(serde_json::to_string(v).unwrap_or_default()),
        None => existing.input_schema,       // absent → keep
    };
    let output_schema_json = match body.output_schema {
        Some(None) => None,
        Some(Some(ref v)) => Some(serde_json::to_string(v).unwrap_or_default()),
        None => existing.output_schema,
    };
    let namespace = match body.namespace {
        Some(None) => None,
        Some(Some(ref v)) => Some(v.clone()),
        None => existing.namespace,
    };

    if !path_prefix.starts_with('/') {
        return Err(AppError::Validation("path_prefix must start with '/'".into()));
    }

    sqlx::query(
        "UPDATE compositions SET name = ?1, path_prefix = ?2, path_pattern = ?3, methods = ?4, host_pattern = ?5, timeout_ms = ?6, max_wait_ms = ?7, auth_skip = ?8, active = ?9, response_merge = ?10, input_schema = ?11, output_schema = ?12, namespace = ?13, updated_at = datetime('now') WHERE id = ?14",
    )
    .bind(&name)
    .bind(&path_prefix)
    .bind(&path_pattern)
    .bind(&methods_json)
    .bind(&host_pattern)
    .bind(timeout_ms)
    .bind(max_wait_ms)
    .bind(auth_skip)
    .bind(active)
    .bind(&response_merge_json)
    .bind(&input_schema_json)
    .bind(&output_schema_json)
    .bind(&namespace)
    .bind(id.to_string())
    .execute(&pool)
    .await?;

    if let Some(new_steps) = body.steps {
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

        sqlx::query("DELETE FROM composition_steps WHERE composition_id = ?1")
            .bind(id.to_string())
            .execute(&pool)
            .await?;

        for (i, step) in new_steps.iter().enumerate() {
            let step_id = Uuid::new_v4().to_string();
            let body_tmpl = step.body_template.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
            let headers_tmpl = step.headers_template.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
            let depends_on_json = step.depends_on.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());
            let default_val = step.default_value.as_ref().map(|v| serde_json::to_string(v).unwrap_or_default());

            sqlx::query(
                "INSERT INTO composition_steps (id, composition_id, name, step_order, method, upstream_id, path_template, body_template, headers_template, depends_on, on_error, default_value, timeout_ms, use_internal_route) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            )
            .bind(&step_id)
            .bind(id.to_string())
            .bind(step.name.trim())
            .bind(step.step_order.unwrap_or(i as i32))
            .bind(step.method.as_deref().unwrap_or("GET"))
            .bind(step.upstream_id.to_string())
            .bind(&step.path_template)
            .bind(&body_tmpl)
            .bind(&headers_tmpl)
            .bind(&depends_on_json)
            .bind(step.on_error.as_deref().unwrap_or("abort"))
            .bind(&default_val)
            .bind(step.timeout_ms.unwrap_or(10000))
            .bind(step.use_internal_route.unwrap_or(false))
            .execute(&pool)
            .await?;
        }
    }

    let comp_row: CompositionRow = sqlx::query_as("SELECT * FROM compositions WHERE id = ?1")
        .bind(id.to_string())
        .fetch_one(&pool)
        .await?;

    let step_rows: Vec<StepRow> = sqlx::query_as(
        "SELECT * FROM composition_steps WHERE composition_id = ?1 ORDER BY step_order",
    )
    .bind(id.to_string())
    .fetch_all(&pool)
    .await?;

    Ok(Json(CompositionWithSteps {
        composition: CompositionResponse::from(comp_row),
        steps: step_rows.into_iter().map(StepResponse::from).collect(),
    }))
}

pub async fn get_composition_openapi(
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let comp_row: CompositionRow = sqlx::query_as("SELECT * FROM compositions WHERE id = ?1")
        .bind(id.to_string())
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound("Composition not found".into()))?;

    let comp = CompositionResponse::from(comp_row);
    let spec = build_openapi_spec(&comp);
    Ok(Json(spec))
}

fn build_openapi_spec(comp: &CompositionResponse) -> serde_json::Value {
    let methods_val = comp.methods.as_ref().and_then(|v| v.as_array());
    let method = methods_val
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_lowercase())
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

#[derive(sqlx::FromRow)]
struct NsCountRow {
    namespace: Option<String>,
    count: i64,
}

pub async fn list_namespaces(
    State(pool): State<SqlitePool>,
) -> Result<Json<Vec<NamespaceSummary>>, AppError> {
    let rows: Vec<NsCountRow> = sqlx::query_as(
        "SELECT namespace, COUNT(*) as count FROM compositions GROUP BY namespace ORDER BY CASE WHEN namespace IS NULL THEN 1 ELSE 0 END, namespace",
    )
    .fetch_all(&pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(|r| NamespaceSummary { namespace: r.namespace, count: r.count })
            .collect(),
    ))
}

pub async fn get_namespace_openapi(
    State(pool): State<SqlitePool>,
    Path(ns): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows: Vec<CompositionRow> = if ns == "_ungrouped" {
        sqlx::query_as("SELECT * FROM compositions WHERE namespace IS NULL ORDER BY created_at")
            .fetch_all(&pool)
            .await?
    } else {
        sqlx::query_as("SELECT * FROM compositions WHERE namespace = ?1 ORDER BY created_at")
            .bind(&ns)
            .fetch_all(&pool)
            .await?
    };

    if rows.is_empty() {
        return Err(AppError::NotFound("Namespace not found or empty".into()));
    }

    let compositions: Vec<CompositionResponse> = rows.into_iter().map(CompositionResponse::from).collect();
    let spec = build_combined_openapi_spec(&ns, &compositions);
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
    State(pool): State<SqlitePool>,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let result = sqlx::query("DELETE FROM compositions WHERE id = ?1")
        .bind(id.to_string())
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Composition not found".into()));
    }

    Ok(axum::http::StatusCode::NO_CONTENT)
}
