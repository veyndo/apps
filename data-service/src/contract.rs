//! One strict operation contract shared by HTTP, MCP, Fetch and the generic Veyndo SDK.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Read {
    pub app: String,
    pub action: String,
    pub query: Query,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<i64>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub after: Option<String>,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
}
pub(crate) fn present<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(d).map(Some)
}
impl Query {
    pub fn value(&self) -> Value {
        serde_json::to_value(self).expect("typed query serialization")
    }
}
pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["app","action","query"],"properties":{"app":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-zA-Z0-9_-]+$"},"action":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[a-zA-Z0-9_-]+$"},"query":{"type":"object","additionalProperties":false,"properties":{"limit":{"type":"integer","minimum":1,"maximum":100,"default":100},"after":{"type":"string","maxLength":100},"id":{"type":"string","minLength":1,"maxLength":100}}}}})
}
pub fn tool() -> Value {
    json!({"name":"apps.records.read","description":"Read declared fields of one currently installed private App list action using current merchant permissions","inputSchema":schema(),"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})
}
pub fn document(writes: bool, settings: bool, installs: bool, jobs: bool) -> Value {
    if jobs {
        let mut doc = document(writes, settings, installs, false);
        doc["version"] = json!(5);
        doc["jobs"] = json!(true);
        for op in ["list", "detail", "control"] {
            doc["operations"].as_array_mut().unwrap().push(
                json!({"name":format!("apps.jobs.{op}"),"schema":crate::job_contract::schema(op)}),
            );
        }
        return doc;
    }
    if installs {
        let mut doc = document(writes, settings, false, false);
        doc["version"] = json!(4);
        doc["installation"] = json!(true);
        for install in [false, true] {
            doc["operations"].as_array_mut().unwrap().push(json!({"name":if install{"apps.packages.install"}else{"apps.packages.review"},"schema":crate::install_contract::schema(install)}));
        }
        return doc;
    }
    if settings {
        let mut doc = document(writes, false, false, false);
        doc["version"] = json!(3);
        doc["settings"] = json!(true);
        for save in [false, true] {
            doc["operations"].as_array_mut().unwrap().push(json!({"name":if save {"apps.settings.save"}else{"apps.settings.read"},"schema":crate::settings_contract::schema(save)}));
        }
        return doc;
    }
    if writes {
        return json!({"version":2,"service":"veyndo-app-data","operations":[{"name":"apps.records.read","schema":schema()},{"name":"apps.records.write","schema":crate::write_contract::schema()},{"name":"apps.records.history","schema":crate::write_contract::history_schema()}],"writes":true,"profile":"portable-private-records"});
    }
    json!({"version":1,"service":"veyndo-app-data","operation":"apps.records.read","schema":schema(),"authority":"fresh current merchant OAuth context; signed read-only PostgreSQL scope","writes":false})
}
fn read_openapi() -> Value {
    json!({"openapi":"3.1.0","info":{"title":"Veyndo App Data","version":env!("CARGO_PKG_VERSION")},"paths":{"/v1/records/query":{"post":{"operationId":"apps.records.read","security":[{"bearer":[]}],"requestBody":{"required":true,"content":{"application/json":{"schema":schema()}}},"responses":{"200":{"description":"Bounded declared-field record page"},"400":{"description":"Invalid query or installed action schema"},"401":{"description":"Inactive or invalid token"},"403":{"description":"Current permission or MCP grant denied"},"404":{"description":"Installed app or action unavailable"},"422":{"description":"A native object resolver is required"},"503":{"description":"Identity or data service unavailable"}}}}},"components":{"securitySchemes":{"bearer":{"type":"http","scheme":"bearer"}}}})
}

pub fn openapi(writes: bool, settings: bool, installs: bool, jobs: bool) -> Value {
    let mut doc = read_openapi();
    if writes {
        for (path, name, schema) in [
            (
                "/v1/records/write",
                "apps.records.write",
                crate::write_contract::schema(),
            ),
            (
                "/v1/records/history",
                "apps.records.history",
                crate::write_contract::history_schema(),
            ),
        ] {
            doc["paths"][path] = json!({"post":{"operationId":name,"security":[{"bearer":[]}],"requestBody":{"required":true,"content":{"application/json":{"schema":schema}}},"responses":{"200":{"description":"Managed record result"},"400":{"description":"Invalid input or lifecycle"},"403":{"description":"Current authority or MCP denied"},"409":{"description":"Revision or package conflict"},"422":{"description":"Native policy required"},"503":{"description":"No committed result could be confirmed"}}}});
        }
    }
    if settings {
        for save in [false, true] {
            let path = if save {
                "/v1/settings/save"
            } else {
                "/v1/settings/read"
            };
            doc["paths"][path] = json!({"post":{"operationId":if save {"apps.settings.save"}else{"apps.settings.read"},"security":[{"bearer":[]}],"requestBody":{"required":true,"content":{"application/json":{"schema":crate::settings_contract::schema(save)}}},"responses":{"200":{"description":"Secret-safe settings result"},"400":{"description":"Invalid declared settings"},"403":{"description":"Personal live administrator required"},"409":{"description":"Package or settings revision changed"},"503":{"description":"Encrypted settings unavailable; check revision before retrying"}}}});
        }
    }
    if installs {
        for install in [false, true] {
            let path = if install {
                "/v1/packages/install"
            } else {
                "/v1/packages/review"
            };
            doc["paths"][path] = json!({"post":{"operationId":if install{"apps.packages.install"}else{"apps.packages.review"},"security":[{"bearer":[]}],"requestBody":{"required":true,"content":{"application/json":{"schema":crate::install_contract::schema(install)}}},"responses":{"200":{"description":"Reviewed package or atomic installation"},"400":{"description":"Invalid package or incomplete approval"},"403":{"description":"Personal live administrator required"},"409":{"description":"Package/model/version changed"},"422":{"description":"Unsupported host policy"},"503":{"description":"Storage unavailable; review current revision before retry"}}}});
        }
    }
    if jobs {
        for op in ["list", "detail", "control"] {
            doc["paths"][format!("/v1/jobs/{op}")] = json!({"post":{"operationId":format!("apps.jobs.{op}"),"security":[{"bearer":[]}],"requestBody":{"required":true,"content":{"application/json":{"schema":crate::job_contract::schema(op)}}},"responses":{"200":{"description":"Scoped job supervision result"},"400":{"description":"Invalid job query or approval"},"403":{"description":"Current authority denied"},"404":{"description":"Job unavailable"},"409":{"description":"Job/package revision or outcome review required"},"503":{"description":"Unconfirmed result; reload before retrying"}}}});
        }
    }
    doc
}
