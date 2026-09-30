use utoipa::{
    Modify, OpenApi,
    openapi::{
        OpenApi as Document, RefOr,
        security::{Http, HttpAuthScheme, SecurityScheme},
    },
};

/// The Rust DTO derives and route annotations are the contract source of truth.
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::auth::handlers::signup, crate::auth::handlers::login,
        crate::auth::handlers::logout, crate::auth::handlers::me,
        crate::channels::handlers::create_route, crate::channels::handlers::list_route,
        crate::messages::handlers::post_route, crate::messages::handlers::history_route
    ),
    modifiers(&BearerSecurity, &ErrorCodes),
    info(title = "Hamlet HTTP API", version = "1.0.0")
)]
pub struct ApiDoc;

struct BearerSecurity;
impl Modify for BearerSecurity {
    fn modify(&self, api: &mut Document) {
        api.components
            .get_or_insert_with(Default::default)
            .add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
            );
    }
}

/// Error status/code pairs are shared across routes by the common HTTP mapper.
struct ErrorCodes;
impl Modify for ErrorCodes {
    fn modify(&self, api: &mut Document) {
        for path in api.paths.paths.values_mut() {
            for operation in [path.get.as_mut(), path.post.as_mut()]
                .into_iter()
                .flatten()
            {
                for (status, response) in &mut operation.responses.responses {
                    let code = match status.as_str() {
                        "400" => "bad_request",
                        "401" => "unauthorized",
                        "404" => "not_found",
                        "409" => "conflict",
                        "500" => "internal_error",
                        _ => continue,
                    };
                    if let RefOr::T(response) = response {
                        response.description = format!("error.code = {code}");
                    }
                }
            }
        }
    }
}

pub fn generated_json() -> String {
    let mut document = serde_json::to_value(ApiDoc::openapi()).expect("OpenAPI serialization");
    // Constrain each status's code in JSON Schema as well as describing it in prose.
    // The referenced ErrorBody/Info schemas still come from the Rust DTOs.
    for path in document["paths"]
        .as_object_mut()
        .expect("paths")
        .values_mut()
    {
        for operation in path.as_object_mut().expect("operations").values_mut() {
            for (status, response) in operation["responses"].as_object_mut().expect("responses") {
                let code = match status.as_str() {
                    "400" => "bad_request",
                    "401" => "unauthorized",
                    "404" => "not_found",
                    "409" => "conflict",
                    "500" => "internal_error",
                    _ => continue,
                };
                response["content"]["application/json"]["schema"] = serde_json::json!({
                    "allOf": [
                        {"$ref": "#/components/schemas/ErrorBody"},
                        {"type": "object", "properties": {"error": {"type": "object", "properties": {
                            "code": {"const": code}
                        }}}}
                    ]
                });
            }
        }
    }
    format!(
        "{}\n",
        serde_json::to_string_pretty(&document).expect("OpenAPI serialization")
    )
}
