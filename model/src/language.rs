//! Enabled content-language and main-language requirements with the original native inheritance rules.
use crate::{Entity, Result, bad};
use serde_json::Value;
pub fn validate(e: &Entity, fields: &Value, locales: &[String], main_locale: &str) -> Result<()> {
    for f in e
        .fields
        .iter()
        .filter(|f| f.translatable || f.kind == "richtext")
    {
        let Some(map) = fields[&f.name].as_object() else {
            continue;
        };
        if map.keys().any(|k| {
            !locales
                .iter()
                .any(|l| l == k || l.split('-').next() == Some(k.as_str()))
        }) {
            return Err(bad("App translation language is not enabled for this shop"));
        }
        if f.kind == "richtext" {
            if f.required
                && fields[&f.name][main_locale]
                    .as_array()
                    .or_else(|| fields[&f.name][main_locale.split('-').next().unwrap()].as_array())
                    .is_none()
            {
                return Err(bad("Required rich field needs main-language content"));
            }
            continue;
        }
        if f.required
            && fields[&f.name][main_locale]
                .as_str()
                .or_else(|| fields[&f.name][main_locale.split('-').next().unwrap()].as_str())
                .is_none()
        {
            return Err(bad("Required app field needs a main-language value"));
        }
    }
    Ok(())
}
