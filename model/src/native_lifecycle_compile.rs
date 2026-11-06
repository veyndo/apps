//! Compile bounded app-local state graphs into the native machine; conflicts and stranded paths fail closed.
use crate::{Result, bad, conflict, declarations::identifier, native_lifecycle::*};
use serde_json::{Value, json};
use std::collections::HashMap;
const ORDER_GUARDS: &[&str] = &["payment.ready", "delivery.delivered"];
const QUOTE_GUARDS: &[&str] = &["quote.message"];
const QUOTE_FROM: &[&str] = &["requested"];
const QUOTE_TO: &[&str] = &["requested", "rejected"];
const REPLACEMENTS: &[&str] = &["start", "complete"];
/// Read-only generation guidance shares the exact native graph/guard/scope owners.
pub fn generation_context<P: CompilerPolicy>(policy: &P) -> Value {
    json!({"version":1,"order":{"machine":policy.order_base(),"guards":ORDER_GUARDS,"replaceableActions":REPLACEMENTS},"quote":{"machine":quote_machine(),"allowedNativeFrom":QUOTE_FROM,"allowedNativeTo":QUOTE_TO,"guards":QUOTE_GUARDS,"replaceableActions":[]},"requiredPermissions":policy.scopes()})
}
fn key(app: &str, id: &str) -> String {
    use sha2::Digest;
    let digest = format!("{:x}", sha2::Sha256::digest(app.as_bytes()));
    format!("a{}_{}", &digest[..8], id)
}
fn labels<P: CompilerPolicy>(v: &HashMap<String, String>, policy: &P) -> bool {
    !v.is_empty()
        && v.len() <= 40
        && v.iter()
            .all(|(l, s)| policy.locale(l) && !s.trim().is_empty() && s.len() <= 100)
}
pub fn quote_machine() -> Machine {
    Machine {
        states: ["requested", "offered", "accepted", "ordered", "rejected"]
            .into_iter()
            .map(|id| MachineState {
                id: id.into(),
                label: HashMap::new(),
                terminal: ["ordered", "rejected"].contains(&id),
            })
            .collect(),
        transitions: vec![],
        source_app: None,
    }
}
pub fn compile<P: CompilerPolicy>(
    mut base: Machine,
    apps: &[Package<'_>],
    object: &str,
    policy: &P,
) -> Result<Machine> {
    let initial = if object == "order" {
        "placed"
    } else {
        "requested"
    };
    let builtin = base.states.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
    let mut replacements = std::collections::HashSet::new();
    for m in apps {
        for c in m.lifecycles.iter().filter(|c| c.object == object) {
            let names = c
                .states
                .iter()
                .map(|s| s.id.as_str())
                .collect::<std::collections::HashSet<_>>();
            if names.len() != c.states.len()
                || c.states.iter().any(|s| {
                    !identifier(&s.id)
                        || s.id.len() > 16
                        || builtin.contains(&s.id)
                        || !labels(&s.label, policy)
                })
            {
                return Err(bad(
                    "Lifecycle states require unique local IDs and translated labels",
                ));
            }
            let resolve = |s: &str| -> Result<String> {
                if names.contains(s) {
                    Ok(key(m.id, s))
                } else if builtin.iter().any(|id| id == s) {
                    Ok(s.into())
                } else {
                    Err(bad("Lifecycle edge references an undeclared state"))
                }
            };
            for state in &c.states {
                let id = key(m.id, &state.id);
                if base.states.iter().any(|s| s.id == id) {
                    return Err(bad("Lifecycle state collision"));
                }
                base.states.push(MachineState {
                    id,
                    label: state.label.clone(),
                    terminal: false,
                });
            }
            for e in &c.transitions {
                let from = resolve(&e.from)?;
                let to = resolve(&e.to)?;
                if !identifier(&e.id) || e.id.len() > 16 || !labels(&e.label, policy) {
                    return Err(bad(
                        "Lifecycle actions require short unique IDs and translated labels",
                    ));
                }
                if e.required_permissions.len() > 8
                    || e.required_permissions
                        .iter()
                        .any(|p| !policy.scopes().contains(&p.as_str()))
                {
                    return Err(bad("Unknown native lifecycle permission"));
                }
                if e.guards.len() > 4
                    || e.guards.iter().any(|g| !match object {
                        "order" => ORDER_GUARDS.contains(&g.as_str()),
                        _ => QUOTE_GUARDS.contains(&g.as_str()),
                    })
                {
                    return Err(bad("Unsupported guard for this native lifecycle"));
                }
                if from == to
                    || base.states.iter().any(|s| s.id == from && s.terminal)
                    || object == "order"
                        && (["expired", "payment_review"].contains(&to.as_str())
                            || base.states.iter().any(|s| {
                                s.id == to
                                    && s.terminal
                                    && !["completed", "cancelled"].contains(&s.id.as_str())
                            }))
                    || object == "quote"
                        && (e.replaces.is_some()
                            || !QUOTE_FROM.contains(&e.from.as_str())
                                && !names.contains(e.from.as_str())
                            || !QUOTE_TO.contains(&e.to.as_str()) && !names.contains(e.to.as_str()))
                {
                    return Err(bad("Unsupported native lifecycle transition"));
                }
                if let Some(replace) = &e.replaces {
                    if !REPLACEMENTS.contains(&replace.as_str())
                        || !replacements.insert(replace.clone())
                    {
                        return Err(conflict("Apps compete for a native lifecycle transition"));
                    }
                    let index = base
                        .transitions
                        .iter()
                        .position(|edge| {
                            edge.id == *replace && edge.from == from && edge.policy.is_none()
                        })
                        .ok_or(bad(
                            "Replaced native transition must start at the same state",
                        ))?;
                    base.transitions.remove(index);
                }
                if base
                    .transitions
                    .iter()
                    .any(|p| p.id == key(m.id, &e.id) || p.from == from && p.to == to)
                {
                    return Err(bad("Duplicate lifecycle transition"));
                }
                base.transitions.push(MachineEdge {
                    id: key(m.id, &e.id),
                    from,
                    to,
                    label: e.label.clone(),
                    policy: Some(Policy {
                        app: m.id.to_owned(),
                        version: m.version.to_owned(),
                        digest: m.digest.to_owned(),
                        required_permissions: e.required_permissions.clone(),
                        guards: e.guards.clone(),
                    }),
                });
            }
        }
    }
    if base.states.len() > 32 || base.transitions.len() > 100 {
        return Err(bad("Combined native lifecycle exceeds limits"));
    }
    let mut reachable = std::collections::HashSet::from([initial.to_owned()]);
    let mut finish = builtin
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    for _ in 0..base.states.len() {
        for edge in &base.transitions {
            if reachable.contains(&edge.from) {
                reachable.insert(edge.to.clone());
            }
            if finish.contains(&edge.to) {
                finish.insert(edge.from.clone());
            }
        }
    }
    if base.states.iter().any(|s| {
        !builtin.contains(&s.id) && (!reachable.contains(&s.id) || !finish.contains(&s.id))
    }) {
        return Err(bad(
            "Every app state must be reachable and lead back to a native state",
        ));
    }
    if json!(&base).to_string().len() > 60_000 {
        return Err(bad("Combined lifecycle snapshot exceeds 60 KiB"));
    }
    Ok(base)
}
