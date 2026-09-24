use actix_web::{
    Error, HttpMessage, HttpResponse, Responder,
    dev::ServiceRequest,
    http::StatusCode,
    middleware::{Next, from_fn},
    web,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand::RngCore;
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement,
};
mod auth;
mod channels;
pub mod contract;
mod messages;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration as StdDuration;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    cursor_key: String,
}

pub async fn connect(url: &str) -> Result<AppState, String> {
    if !url.starts_with("sqlite:") {
        return Err("rewrite requires a SQLite URL".into());
    }
    let mut opts = ConnectOptions::new(url);
    let file_backed = !url.contains(":memory:");
    opts.max_connections(if file_backed { 5 } else { 1 })
        .min_connections(1)
        .connect_timeout(StdDuration::from_secs(5))
        .sqlx_logging(false)
        .map_sqlx_sqlite_opts(move |options| {
            let options = options
                .foreign_keys(true)
                .busy_timeout(StdDuration::from_secs(5));
            if file_backed {
                options.journal_mode(sea_orm::sqlx::sqlite::SqliteJournalMode::Wal)
            } else {
                options
            }
        });
    let db = Database::connect(opts)
        .await
        .map_err(|e| format!("rewrite database connection failed: {e}"))?;
    use sea_orm_migration::MigratorTrait;
    hamlet_rewrite_migration::Migrator::up(&db, None)
        .await
        .map_err(|e| format!("rewrite migration failed: {e}"))?;
    channels::bootstrap(&db)
        .await
        .map_err(|e| format!("channel bootstrap failed: {e}"))?;
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT OR IGNORE INTO cursor_keys (id, secret) VALUES (1, ?)",
        [new_token().into()],
    ))
    .await
    .map_err(|e| format!("cursor key initialization failed: {e}"))?;
    let row = db
        .query_one_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT secret FROM cursor_keys WHERE id = 1",
        ))
        .await
        .map_err(|e| format!("cursor key lookup failed: {e}"))?
        .ok_or("cursor key missing")?;
    let cursor_key = row
        .try_get("", "secret")
        .map_err(|e| format!("cursor key malformed: {e}"))?;
    Ok(AppState { db, cursor_key })
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct User {
    pub id: String,
    pub username: String,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct AuthResponse {
    pub user: User,
    pub access_token: String,
    pub expires_at: DateTime<Utc>,
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}
#[derive(Serialize, utoipa::ToSchema)]
struct ErrorBody {
    error: ErrorInfo,
}
#[derive(Serialize, utoipa::ToSchema)]
struct ErrorInfo {
    code: &'static str,
    message: &'static str,
}

fn problem(status: StatusCode, code: &'static str, message: &'static str) -> HttpResponse {
    HttpResponse::build(status).json(ErrorBody {
        error: ErrorInfo { code, message },
    })
}
fn bad_request() -> HttpResponse {
    problem(StatusCode::BAD_REQUEST, "bad_request", "Invalid request")
}
fn internal() -> HttpResponse {
    problem(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "Internal server error",
    )
}
fn unauthorized() -> HttpResponse {
    problem(StatusCode::UNAUTHORIZED, "unauthorized", "Unauthorized")
}
fn digest(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}
fn new_id() -> i64 {
    100_000_000_000_000 + (rand::random::<u64>() % 900_000_000_000_000) as i64
}
fn new_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}
fn valid_username(name: &str) -> bool {
    (3..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.')
}
fn valid_password(password: &str) -> bool {
    (8..=256).contains(&password.len())
}

#[utoipa::path(post, path = "/api/v1/auth/signup", request_body = Credentials,
    responses((status = 201, body = AuthResponse), (status = 400, body = ErrorBody),
        (status = 409, body = ErrorBody), (status = 500, body = ErrorBody)))]
pub(crate) async fn signup(
    db: web::Data<AppState>,
    input: Result<web::Json<Credentials>, Error>,
) -> impl Responder {
    let Ok(input) = input else {
        return bad_request();
    };
    match auth::register(&db.db, &input.username, &input.password).await {
        Ok(response) => HttpResponse::Created().json(response),
        Err(auth::SignupError::Invalid) => bad_request(),
        Err(auth::SignupError::Duplicate) => {
            problem(StatusCode::CONFLICT, "conflict", "Username already exists")
        }
        Err(auth::SignupError::Internal) => internal(),
    }
}

#[utoipa::path(post, path = "/api/v1/auth/login", request_body = Credentials,
    responses((status = 200, body = AuthResponse), (status = 400, body = ErrorBody),
        (status = 401, body = ErrorBody), (status = 500, body = ErrorBody)))]
pub(crate) async fn login(
    db: web::Data<AppState>,
    input: Result<web::Json<Credentials>, Error>,
) -> impl Responder {
    let Ok(input) = input else {
        return bad_request();
    };
    match auth::login(&db.db, &input.username, &input.password).await {
        Ok(response) => HttpResponse::Ok().json(response),
        Err(auth::LoginError::Invalid) => unauthorized(),
        Err(auth::LoginError::Internal) => internal(),
    }
}

#[utoipa::path(post, path = "/api/v1/auth/logout", security(("bearer_auth" = [])),
    responses((status = 204), (status = 401, body = ErrorBody), (status = 500, body = ErrorBody)))]
pub(crate) async fn logout(db: web::Data<AppState>, req: actix_web::HttpRequest) -> impl Responder {
    let identity = req
        .extensions()
        .get::<Identity>()
        .cloned()
        .expect("protected scope");
    match auth::logout(&db.db, &identity.token_digest).await {
        Ok(()) => HttpResponse::NoContent().finish(),
        Err(_) => internal(),
    }
}

#[derive(Clone)]
pub struct Identity {
    pub user: UserIdentity,
    pub token_digest: String,
}
#[derive(Clone)]
pub struct UserIdentity {
    pub id: i64,
    pub username: String,
}

async fn bearer(
    req: ServiceRequest,
    next: Next<impl actix_web::body::MessageBody + 'static>,
) -> Result<actix_web::dev::ServiceResponse<actix_web::body::BoxBody>, Error> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .filter(|t| !t.is_empty() && !t.contains(' '));
    let identity = if let (Some(token), Some(db)) = (token, req.app_data::<web::Data<AppState>>()) {
        match db.db.query_one_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "SELECT u.id, u.username, s.expires_at FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_digest = ?",
            [digest(token).into()])).await {
            Ok(Some(row)) => {
                let fields = (|| -> Option<(i64, String, DateTime<chrono::FixedOffset>)> {
                    Some((row.try_get("", "id").ok()?, row.try_get("", "username").ok()?,
                        DateTime::parse_from_rfc3339(&row.try_get::<String>("", "expires_at").ok()?).ok()?))
                })();
                match fields {
                    Some((id, username, expiry)) if expiry > Utc::now() => Some(Identity {
                        user: UserIdentity { id, username }, token_digest: digest(token),
                    }),
                    Some(_) => None,
                    None => return Ok(req.into_response(internal())),
                }
            }
            Ok(None) => None,
            Err(_) => return Ok(req.into_response(internal())),
        }
    } else {
        None
    };
    match identity {
        Some(identity) => {
            req.extensions_mut().insert(identity);
            Ok(next.call(req).await?.map_into_boxed_body())
        }
        None => Ok(req.into_response(unauthorized())),
    }
}
async fn request_id(
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

#[utoipa::path(get, path = "/api/v1/me", security(("bearer_auth" = [])),
    responses((status = 200, body = User), (status = 401, body = ErrorBody),
        (status = 500, body = ErrorBody)))]
pub(crate) async fn me(req: actix_web::HttpRequest) -> impl Responder {
    let identity = req
        .extensions()
        .get::<Identity>()
        .cloned()
        .expect("protected scope");
    HttpResponse::Ok().json(User {
        id: identity.user.id.to_string(),
        username: identity.user.username,
    })
}

pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.app_data(web::JsonConfig::default().error_handler(|_, _| {
        actix_web::error::InternalError::from_response("json", bad_request()).into()
    }));
    cfg.service(
        web::scope("")
            .wrap(from_fn(request_id))
            .service(
                web::scope("/api/v1")
                    .service(
                        web::resource("/auth/signup")
                            .route(web::post().to(signup))
                            .default_service(web::to(|| async {
                                problem(
                                    StatusCode::METHOD_NOT_ALLOWED,
                                    "method_not_allowed",
                                    "Method not allowed",
                                )
                            })),
                    )
                    .service(
                        web::resource("/auth/login")
                            .route(web::post().to(login))
                            .default_service(web::to(|| async {
                                problem(
                                    StatusCode::METHOD_NOT_ALLOWED,
                                    "method_not_allowed",
                                    "Method not allowed",
                                )
                            })),
                    )
                    .service(
                        web::scope("")
                            .wrap(from_fn(bearer))
                            .service(
                                web::resource("/auth/logout")
                                    .route(web::post().to(logout))
                                    .default_service(web::to(|| async {
                                        problem(
                                            StatusCode::METHOD_NOT_ALLOWED,
                                            "method_not_allowed",
                                            "Method not allowed",
                                        )
                                    })),
                            )
                            .service(
                                web::resource("/me")
                                    .route(web::get().to(me))
                                    .default_service(web::to(|| async {
                                        problem(
                                            StatusCode::METHOD_NOT_ALLOWED,
                                            "method_not_allowed",
                                            "Method not allowed",
                                        )
                                    })),
                            )
                            .configure(channels::routes)
                            .configure(messages::routes),
                    )
                    .default_service(web::to(|| async {
                        problem(StatusCode::NOT_FOUND, "not_found", "Not found")
                    })),
            )
            .default_service(web::to(|| async {
                problem(StatusCode::NOT_FOUND, "not_found", "Not found")
            })),
    );
}
