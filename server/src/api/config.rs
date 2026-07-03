//! Public, safe-to-expose server settings for unauthenticated clients.

use actix_web::{Responder, get, web};
use serde::Serialize;

use crate::Config;

#[derive(Debug, Serialize)]
struct PublicConfigResponse {
    account_registration_enabled: bool,
}

impl From<&Config> for PublicConfigResponse {
    fn from(config: &Config) -> Self {
        Self {
            account_registration_enabled: config.account_registration_enabled,
        }
    }
}

#[get("/config")]
async fn get_public_config(config: web::Data<Config>) -> impl Responder {
    let account_registration_enabled = config.account_registration_enabled;
    web::Json(PublicConfigResponse {
        account_registration_enabled,
    })
}

pub fn configure_public(cfg: &mut web::ServiceConfig) {
    cfg.service(get_public_config);
}
