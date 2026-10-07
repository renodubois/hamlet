use crate::new_id;
use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

/// Seeds a general text channel only when no active conversation remains.
pub(crate) async fn bootstrap(db: &DatabaseConnection) -> Result<(), DbErr> {
    for _ in 0..5 {
        let result = db.execute_raw(Statement::from_sql_and_values(DbBackend::Sqlite,
            "INSERT INTO channels (id, name, name_key, type) SELECT ?, 'general', 'general', 'text' WHERE NOT EXISTS (SELECT 1 FROM channels WHERE deleted_at IS NULL)",
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
