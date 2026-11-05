//! Namespaced graph projections over already-authorized app rows; no storage, identity resolution or confirmed-claim authority.
use crate::{Entity, Result, bad, declarations::identifier, intelligence::Context};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Mapping {
    pub entity: String,
    pub node_type: String,
    pub label: HashMap<String, String>,
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub relations: HashMap<String, String>,
}
pub fn validate(m: &Context<'_>) -> Result<()> {
    let Some(ai) = &m.intelligence else {
        return Ok(());
    };
    let mut entities = std::collections::HashSet::new();
    if ai.ontology.len() > 4 {
        return Err(bad("Invalid app ontology mapping"));
    }
    for node in &ai.ontology {
        let entity = m
            .entities
            .iter()
            .find(|e| e.name == node.entity)
            .ok_or(bad("Invalid app ontology mapping"))?;
        let mut fields = std::collections::HashSet::new();
        if !identifier(&node.node_type)
            || !entities.insert(&node.entity)
            || !ai.entities.contains(&node.entity)
            || !m.permissions.iter().any(|p| p == "data.read")
            || !m
                .actions
                .iter()
                .any(|act| act.handler == "list" && act.entity.as_deref() == Some(&node.entity))
            || node.label.is_empty()
            || node.label.len() > 100
            || node
                .label
                .values()
                .any(|l| l.trim().is_empty() || l.len() > 120)
            || node.relations.iter().any(|(field, kind)| {
                !identifier(kind)
                    || !node.fields.contains(field)
                    || !entity.fields.iter().any(|f| {
                        f.name == *field && (f.core_reference.is_some() || f.references.is_some())
                    })
            })
            || node.fields.is_empty()
            || node.fields.len() > 16
            || node
                .fields
                .iter()
                .any(|f| !fields.insert(f) || !entity.fields.iter().any(|native| native.name == *f))
        {
            return Err(bad("Invalid app ontology mapping"));
        }
    }
    Ok(())
}
/// Called only after the existing native list has applied current action/entity rights and tenant RLS.
pub fn project(tenant: &str, m: &Context<'_>, e: &Entity, rows: &Value) -> Option<Value> {
    let inferred = e.extends.as_ref().map(|_| Mapping {
        entity: e.name.clone(),
        node_type: e.name.clone(),
        label: e.label.clone(),
        fields: e.fields.iter().map(|f| f.name.clone()).collect(),
        relations: HashMap::new(),
    });
    let mapping = m
        .intelligence
        .as_ref()
        .and_then(|ai| ai.ontology.iter().find(|n| n.entity == e.name))
        .or(inferred.as_ref())?;
    let rows = rows.as_array()?;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut bytes = 300;
    for row in rows.iter().take(24) {
        let id = row["id"].as_str()?;
        let source = format!("app.{}.{}:{}", m.id, e.name, id);
        let properties = mapping
            .fields
            .iter()
            .filter_map(|f| row.get(f).map(|v| (f.clone(), v.clone())))
            .collect::<serde_json::Map<_, _>>();
        let node = json!({"id":source,"type":format!("app.{}.{}",m.id,mapping.node_type),"label":mapping.label,"properties":properties,"source":{"tenant":tenant,"app":m.id,"version":m.version,"entity":e.name,"recordId":id,"revision":row["revision"]}});
        let relations = e.fields.iter().filter(|f| mapping.fields.contains(&f.name) || e.extends.as_ref().is_some_and(|x|x.field==f.name)).flat_map(|f| {
            let target = if let Some(core) = &f.core_reference {
                Some(("native-reference", core.clone()))
            } else if let Some(entity) = &f.references {
                let target = m.intelligence.as_ref().and_then(|ai|ai.ontology.iter().find(|n| n.entity == *entity)).map(|n| &n.node_type).unwrap_or(entity);
                Some(("app-reference", format!("app.{}.{}",m.id,target)))
            } else { None };
            let Some((kind,target_type)) = target else { return Vec::new(); };
            let kind = mapping.relations.get(&f.name).map(|kind| format!("app.{}.{}",m.id,kind)).unwrap_or(kind.to_owned());
            let ids = if let Some(ids) = row[&f.name].as_array() { ids.iter().filter_map(Value::as_str).take(100).collect::<Vec<_>>() } else { row[&f.name].as_str().into_iter().collect::<Vec<_>>() };
            ids.into_iter().map(|id|json!({"type":kind,"source":source,"field":f.name,"targetType":target_type,"targetId":id,"tenant":tenant})).collect::<Vec<_>>()
        }).collect::<Vec<_>>();
        let size = node.to_string().len() + json!(relations).to_string().len();
        if bytes + size > 16384 {
            break;
        }
        bytes += size;
        nodes.push(node);
        edges.extend(relations);
    }
    Some(
        json!({"tenant":tenant,"nodes":nodes,"edges":edges,"omittedRecords":rows.len()-nodes.len(),"assurance":"native-app-records-not-confirmed-product-claims","visibility":if e.public_read {"declared-public-records"}else{"merchant-private"}}),
    )
}
