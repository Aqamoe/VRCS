use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use super::{api_error, ApiResult, AppState};

pub(super) async fn test_image(State(state): State<Arc<AppState>>) -> ApiResult<Json<Value>> {
    let answer = crate::vision::VisionService::new(state)
        .test_image()
        .await
        .map_err(|detail| {
            api_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "vision.test_failed",
                detail,
            )
        })?;
    Ok(Json(json!({ "answer": answer })))
}
