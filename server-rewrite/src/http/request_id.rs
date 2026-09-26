use crate::new_token;
use actix_web::{Error, dev::ServiceRequest, middleware::Next};

pub(crate) async fn request_id(
    req: ServiceRequest,
    next: Next<impl actix_web::body::MessageBody>,
) -> Result<actix_web::dev::ServiceResponse<impl actix_web::body::MessageBody>, Error> {
    let id = new_token();
    let method = req.method().clone();
    let path = req.path().to_owned();
    let mut response = next.call(req).await?;
    response.headers_mut().insert(
        actix_web::http::header::HeaderName::from_static("x-request-id"),
        actix_web::http::header::HeaderValue::from_str(&id).expect("generated ASCII token"),
    );
    tracing::info!(request_id = %id, %method, %path, status = %response.status(), "HTTP request");
    Ok(response)
}
