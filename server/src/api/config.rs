//! Public, safe-to-expose server settings for unauthenticated clients.

use actix_web::{Responder, get, web};
use serde::Serialize;

use crate::config::ServerSettings;

#[derive(Debug, Serialize)]
struct PublicConfigResponse {
    account_registration_enabled: bool,
}

impl From<&ServerSettings> for PublicConfigResponse {
    fn from(settings: &ServerSettings) -> Self {
        Self {
            account_registration_enabled: settings.account_registration_enabled,
        }
    }
}

#[get("/config")]
async fn get_public_config(settings: Option<web::Data<ServerSettings>>) -> impl Responder {
    let account_registration_enabled = settings
        .as_ref()
        .map(|settings| settings.account_registration_enabled)
        .unwrap_or_else(|| ServerSettings::default().account_registration_enabled);
    web::Json(PublicConfigResponse {
        account_registration_enabled,
    })
}

pub fn configure_public(cfg: &mut web::ServiceConfig) {
    cfg.service(get_public_config);
}
