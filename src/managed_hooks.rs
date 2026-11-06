//! Original native lifecycle and bounded history hooks for the shared writer; caller retains full field/reference/package/identity admission.
use crate::{
    RecordKey,
    model::{Entity, manifest::Manifest},
    mutations::WriteHooks,
    record_history,
};
use serde_json::Value;
use sqlx::PgConnection;
pub enum Error {
    Model(crate::model::Error),
    Storage(crate::Error),
}
pub struct Hooks<'a> {
    pub key: RecordKey<'a>,
    pub manifest: &'a Manifest,
    pub entity: &'a Entity,
}
impl Hooks<'_> {
    async fn capture(
        &self,
        tx: &mut PgConnection,
        revision: i64,
        fields: &Value,
    ) -> Result<(), Error> {
        record_history::capture(
            tx,
            &record_history::Capture {
                key: self.key,
                revision,
                fields,
                read_scopes: &record_history::read_scopes(self.manifest, &self.entity.name),
            },
        )
        .await
        .map_err(Error::Storage)
    }
}
impl WriteHooks for Hooks<'_> {
    type Error = Error;
    async fn before(
        &self,
        tx: &mut PgConnection,
        old: Option<&Value>,
        fields: &Value,
        revision: i64,
    ) -> Result<(), Error> {
        crate::model::lifecycle::change(self.entity, old, fields).map_err(Error::Model)?;
        if let Some(old) = old {
            let prior = self
                .entity
                .fields
                .iter()
                .map(|f| (f.name.clone(), old[&f.name].clone()))
                .collect::<Value>();
            self.capture(tx, revision, &prior).await?;
        }
        Ok(())
    }
    async fn after(
        &self,
        tx: &mut PgConnection,
        fields: &Value,
        revision: i64,
    ) -> Result<(), Error> {
        self.capture(tx, revision, fields).await
    }
}
