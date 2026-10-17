//! Compile bounded app-local state graphs into the native machine; conflicts and stranded paths fail closed.
use super::*;
fn key(app: &str, id: &str) -> String {
    format!("a{}_{}", &hash(app)[..8], id)
}
fn labels(v: &HashMap<String, String>) -> bool {
    !v.is_empty()
        && v.len() <= 40
        && v.iter()
            .all(|(l, s)| commerce::valid_locale_key(l) && !s.trim().is_empty() && s.len() <= 100)
}
pub(crate) fn quote_machine() -> commerce::OrderMachine {
    commerce::OrderMachine {
        states: ["requested", "offered", "accepted", "ordered", "rejected"]
            .into_iter()
            .map(|id| commerce::MachineState {
                id: id.into(),
                label: HashMap::new(),
                terminal: ["ordered", "rejected"].contains(&id),
            })
            .collect(),
        transitions: vec![],
        source_app: None,
    }
}
pub(crate) fn compile(
    mut base: commerce::OrderMachine,
    apps: &[Manifest],
    object: &str,
) -> Result<commerce::OrderMachine> {
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
                        || !labels(&s.label)
                })
            {
                return Err(bad(
                    "Lifecycle states require unique local IDs and translated labels",
                ));
            }
            let resolve = |s: &str| -> Result<String> {
                if names.contains(s) {
                    Ok(key(&m.id, s))
                } else if builtin.iter().any(|id| id == s) {
                    Ok(s.into())
                } else {
                    Err(bad("Lifecycle edge references an undeclared state"))
                }
            };
            for state in &c.states {
                let id = key(&m.id, &state.id);
                if base.states.iter().any(|s| s.id == id) {
                    return Err(bad("Lifecycle state collision"));
                }
                base.states.push(commerce::MachineState {
                    id,
                    label: state.label.clone(),
                    terminal: false,
                });
            }
            for e in &c.transitions {
                let from = resolve(&e.from)?;
                let to = resolve(&e.to)?;
                if !identifier(&e.id)
                    || e.id.len() > 16
                    || !labels(&e.label)
                    || from == to
                    || e.required_permissions.len() > 8
                    || e.required_permissions
                        .iter()
                        .any(|p| !auth::SCOPES.contains(&p.as_str()))
                    || e.guards.len() > 4
                    || e.guards.iter().any(|g| !match object {
                        "order" => ["payment.ready", "delivery.delivered"].contains(&g.as_str()),
                        _ => g == "quote.message",
                    })
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
                            || !["requested"].contains(&e.from.as_str())
                                && !names.contains(e.from.as_str())
                            || !["requested", "rejected"].contains(&e.to.as_str())
                                && !names.contains(e.to.as_str()))
                {
                    return Err(bad("Unsupported lifecycle edge, guard or permission"));
                }
                if let Some(replace) = &e.replaces {
                    if !["start", "complete"].contains(&replace.as_str())
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
                    .any(|p| p.id == key(&m.id, &e.id) || p.from == from && p.to == to)
                {
                    return Err(bad("Duplicate lifecycle transition"));
                }
                base.transitions.push(commerce::MachineEdge {
                    id: key(&m.id, &e.id),
                    from,
                    to,
                    label: e.label.clone(),
                    policy: Some(Policy {
                        app: m.id.clone(),
                        version: m.version.clone(),
                        digest: approval::canonical_digest(m),
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
