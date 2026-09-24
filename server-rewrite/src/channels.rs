use crate::{AppState, bad_request, internal, new_id, problem};
use actix_web::{Error, HttpResponse, Responder, http::StatusCode, web};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    Text,
}
impl ChannelType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
        }
    }
    fn parse(value: &str) -> Result<Self, DbErr> {
        match value {
            "text" => Ok(Self::Text),
            _ => Err(DbErr::Custom("unknown channel type".into())),
        }
    }
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateChannel {
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ChannelType,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct Channel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ChannelType,
}
#[derive(Serialize, utoipa::ToSchema)]
pub struct ChannelList {
    pub items: Vec<Channel>,
}

fn valid(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'-' || b == b'_')
}

pub async fn bootstrap(db: &DatabaseConnection) -> Result<(), DbErr> {
    for _ in 0..5 {
        let result = db.execute_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO channels (id, name, name_key, type) SELECT ?, 'general', 'general', 'text' WHERE NOT EXISTS (SELECT 1 FROM channels)",
            [new_id().into()])).await;
        match result {
            Ok(_) => return Ok(()),
            Err(error) if error.to_string().contains("channels.id") => continue,
            Err(error) => return Err(error),
        }
    }
    Err(DbErr::Custom(
        "channel ID collision retry limit exceeded".into(),
    ))
}

pub enum CreateError {
    Invalid,
    Duplicate,
    Internal,
}

pub async fn create(
    db: &DatabaseConnection,
    input: &CreateChannel,
) -> Result<Channel, CreateError> {
    let name = input.name.trim();
    if !valid(name) {
        return Err(CreateError::Invalid);
    }
    for _ in 0..5 {
        let id = new_id();
        match db
            .execute_raw(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "INSERT INTO channels (id, name, name_key, type) VALUES (?, ?, ?, ?)",
                [
                    id.into(),
                    name.to_owned().into(),
                    name.to_ascii_lowercase().into(),
                    input.kind.as_str().into(),
                ],
            ))
            .await
        {
            Ok(_) => {
                return Ok(Channel {
                    id: id.to_string(),
                    name: name.into(),
                    kind: input.kind,
                });
            }
            Err(error) => {
                let msg = error.to_string();
                if msg.contains("channels.name_key") {
                    return Err(CreateError::Duplicate);
                }
                if msg.contains("channels.id") {
                    continue;
                }
                return Err(CreateError::Internal);
            }
        }
    }
    Err(CreateError::Internal)
}

pub async fn list(db: &DatabaseConnection) -> Result<ChannelList, DbErr> {
    let rows = db
        .query_all_raw(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT id, name, type FROM channels ORDER BY name_key ASC, id ASC",
        ))
        .await?;
    let items = rows
        .into_iter()
        .map(|row| -> Result<Channel, DbErr> {
            Ok(Channel {
                id: row.try_get::<i64>("", "id")?.to_string(),
                name: row.try_get("", "name")?,
                kind: ChannelType::parse(&row.try_get::<String>("", "type")?)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ChannelList { items })
}

#[utoipa::path(post, path = "/api/v1/channels", security(("bearer_auth" = [])), request_body = CreateChannel,
    responses((status = 201, body = Channel), (status = 400, body = crate::ErrorBody),
        (status = 401, body = crate::ErrorBody), (status = 409, body = crate::ErrorBody), (status = 500, body = crate::ErrorBody)))]
async fn create_route(
    db: web::Data<AppState>,
    input: Result<web::Json<CreateChannel>, Error>,
) -> impl Responder {
    let Ok(input) = input else {
        return bad_request();
    };
    match create(&db.db, &input).await {
        Ok(channel) => HttpResponse::Created().json(channel),
        Err(CreateError::Invalid) => bad_request(),
        Err(CreateError::Duplicate) => {
            problem(StatusCode::CONFLICT, "conflict", "Channel already exists")
        }
        Err(CreateError::Internal) => internal(),
    }
}

#[utoipa::path(get, path = "/api/v1/channels", security(("bearer_auth" = [])),
    responses((status = 200, body = ChannelList), (status = 401, body = crate::ErrorBody), (status = 500, body = crate::ErrorBody)))]
async fn list_route(db: web::Data<AppState>) -> impl Responder {
    match list(&db.db).await {
        Ok(channels) => HttpResponse::Ok().json(channels),
        Err(_) => internal(),
    }
}

pub fn routes(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::resource("/channels")
            .route(web::post().to(create_route))
            .route(web::get().to(list_route))
            .default_service(web::to(|| async {
                problem(
                    StatusCode::METHOD_NOT_ALLOWED,
                    "method_not_allowed",
                    "Method not allowed",
                )
            })),
    );
}
