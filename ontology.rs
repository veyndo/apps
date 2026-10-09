//! Namespaced graph views over current authorized app records; no duplicated source or public-claim authority.
use super::*;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Mapping {
    pub entity: String,
    pub node_type: String,
    pub label: HashMap<String, String>,
    pub fields: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub relations: HashMap<String, String>,
}
pub(super) fn validate(m: &Manifest) -> Result<()> {
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
pub(super) fn project(tenant: &str, m: &Manifest, e: &Entity, rows: &Value) -> Option<Value> {
    let mapping = m
        .intelligence
        .as_ref()?
        .ontology
        .iter()
        .find(|n| n.entity == e.name)?;
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
        let relations = e.fields.iter().filter(|f| mapping.fields.contains(&f.name)).flat_map(|f| {
            let target = if let Some(core) = &f.core_reference {
                Some(("native-reference", core.clone()))
            } else if let Some(entity) = &f.references {
                let target = m.intelligence.as_ref().unwrap().ontology.iter().find(|n| n.entity == *entity).map(|n| &n.node_type).unwrap_or(entity);
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
#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        serde_json::from_value(json!({"id":"care_graph","version":"0.1.0","coreApi":"1","runtime":"declarative","name":{"en":"Care"},"permissions":["data.read","data.write"],"entities":[{"name":"guides","fields":[{"name":"product_id","kind":"string","coreReference":"product","indexed":true,"required":true},{"name":"care","kind":"string"}]}],"actions":[{"name":"list_guides","description":"Read care guides","handler":"list","entity":"guides","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}],"intelligence":{"description":{"en":"Care"},"tools":["list_guides"],"entities":["guides"],"ontology":[{"entity":"guides","nodeType":"care_advice","label":{"en":"Care advice"},"fields":["product_id","care"]}]}})).unwrap()
    }
    #[test]
    fn contract_uses_native_entities_fields_and_namespaced_types() {
        let m = manifest();
        assert!(super::super::validate(&m).is_ok());
        for change in 0..8 {
            let mut v = serde_json::to_value(&m).unwrap();
            match change {
                0 => v["intelligence"]["ontology"][0]["fields"] = json!(["invented"]),
                1 => v["intelligence"]["ontology"][0]["nodeType"] = json!("product.property"),
                2 => v["intelligence"]["entities"] = json!([]),
                3 => v["intelligence"]["ontology"][0]["label"] = json!({}),
                4 => v["intelligence"]["ontology"][0]["fields"] = json!(["care", "care"]),
                5 => v["permissions"] = json!(["data.write"]),
                6 => v["intelligence"]["ontology"][0]["relations"] = json!({"care":"invented"}),
                _ => {
                    v["intelligence"]["ontology"][0]["relations"] =
                        json!({"product_id":"core.hijack"})
                }
            };
            assert!(validate(&serde_json::from_value(v).unwrap()).is_err());
        }
    }
    #[test]
    fn exact_revision_and_fields_are_native_views_not_duplicate_facts() {
        let mut m = manifest();
        m.intelligence.as_mut().unwrap().ontology[0]
            .relations
            .insert("product_id".into(), "applies_to".into());
        let rows = json!([{"id":"r1","revision":7,"product_id":"mug","care":"Do not machine wash","unselected":"private"}]);
        let graph = project("fixture", &m, &m.entities[0], &rows).unwrap();
        assert_eq!(graph["tenant"], "fixture");
        assert_eq!(graph["nodes"][0]["source"]["tenant"], "fixture");
        assert_eq!(graph["nodes"][0]["type"], "app.care_graph.care_advice");
        assert_eq!(graph["nodes"][0]["source"]["revision"], 7);
        assert_eq!(graph["nodes"][0]["properties"]["care"], rows[0]["care"]);
        assert!(graph["nodes"][0]["properties"].get("unselected").is_none());
        assert_eq!(graph["edges"][0]["targetId"], "mug");
        assert_eq!(graph["edges"][0]["type"], "app.care_graph.applies_to");
        assert_eq!(graph["visibility"], "merchant-private");
        let huge = json!([{"id":"r1","revision":1,"product_id":"mug","care":"x".repeat(20000)}]);
        let bounded = project("fixture", &m, &m.entities[0], &huge).unwrap();
        assert_eq!(bounded["nodes"], json!([]));
        assert_eq!(bounded["omittedRecords"], 1);
    }
    #[test]
    fn legacy_packages_keep_exact_serde_shape_and_no_projection() {
        let mut m = manifest();
        m.intelligence.as_mut().unwrap().ontology.clear();
        assert!(
            serde_json::to_value(&m).unwrap()["intelligence"]
                .get("ontology")
                .is_none()
        );
        assert!(project("fixture", &m, &m.entities[0], &json!([])).is_none());
    }
}
