use axum::{Json, http::StatusCode, response::IntoResponse};
use lyrit_api_model::Problem;
use lyrit_application::ApplicationError;
use uuid::Uuid;

pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    title: &'static str,
    detail: String,
}

impl ApiError {
    pub fn service_unavailable(detail: impl Into<String>) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            code: "service_unavailable",
            title: "Service unavailable",
            detail: detail.into(),
        }
    }
}

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::Validation(detail) => Self {
                status: StatusCode::BAD_REQUEST,
                code: "invalid_request",
                title: "Invalid request",
                detail,
            },
            ApplicationError::Conflict(detail) => Self {
                status: StatusCode::CONFLICT,
                code: "conflict",
                title: "Resource state conflict",
                detail,
            },
            ApplicationError::PayloadTooLarge => Self {
                status: StatusCode::PAYLOAD_TOO_LARGE,
                code: "payload_too_large",
                title: "Payload too large",
                detail: "The uploaded file exceeds the configured size limit.".to_owned(),
            },
            ApplicationError::UnsupportedMedia(detail) => Self {
                status: StatusCode::UNSUPPORTED_MEDIA_TYPE,
                code: "unsupported_media",
                title: "Unsupported media",
                detail,
            },
            ApplicationError::NotFound => Self {
                status: StatusCode::NOT_FOUND,
                code: "not_found",
                title: "Resource not found",
                detail: "The requested resource does not exist.".to_owned(),
            },
            ApplicationError::Repository(_)
            | ApplicationError::InvalidData(_)
            | ApplicationError::Artifact(_)
            | ApplicationError::MediaInspection(_) => Self {
                status: StatusCode::INTERNAL_SERVER_ERROR,
                code: "internal_error",
                title: "Internal error",
                detail: "The request could not be completed. Use the request ID for diagnostics."
                    .to_owned(),
            },
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let problem = Problem {
            r#type: format!("https://lyrit-loom.local/problems/{}", self.code),
            title: self.title.to_owned(),
            status: self.status.as_u16(),
            code: self.code.to_owned(),
            detail: self.detail,
            request_id: Uuid::new_v4(),
        };
        (self.status, Json(problem)).into_response()
    }
}
