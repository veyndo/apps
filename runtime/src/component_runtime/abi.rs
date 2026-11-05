//! Exact, operator-configured WIT namespace compatibility with the same bounded host.
use super::veyndo::commerce::context::Host as ContextHost;
use super::{Extension, Host, Kind, Outcome};
use wasmtime::component::{Component, Linker, TypedFunc};
use wasmtime::{Engine, Store, StoreContextMut};

pub(super) fn namespace(component: &Component, configured: &str) -> Result<String, String> {
    let previous: Vec<_> = configured
        .split(',')
        .filter(|s| !s.trim().is_empty())
        .map(str::trim)
        .collect();
    if previous.len() > 4
        || previous.iter().any(|value| {
            value.len() > 32
                || !value
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                || !value.as_bytes()[0].is_ascii_lowercase()
        })
    {
        return Err("Invalid configured component namespace".into());
    }
    for value in std::iter::once("veyndo").chain(previous) {
        if component
            .get_export_index(None, format!("{value}:commerce/hooks@1.0.0"))
            .is_some()
        {
            return Ok(value.into());
        }
    }
    Err("Unsupported commerce WIT namespace or version".into())
}
pub(super) fn function(
    engine: &Engine,
    component: &Component,
    namespace: &str,
    store: &mut Store<Host>,
) -> wasmtime::Result<TypedFunc<(Kind, String), (Outcome,)>> {
    let mut linker = Linker::new(engine);
    Extension::add_to_linker::<_, wasmtime::component::HasSelf<_>>(&mut linker, |h: &mut Host| h)?;
    if namespace != "veyndo" {
        let mut context = linker.instance(&format!("{namespace}:commerce/context@1.0.0"))?;
        context.func_wrap("cart", |mut cx: StoreContextMut<'_, Host>, (): ()| {
            Ok((ContextHost::cart(cx.data_mut())?,))
        })?;
        context.func_wrap(
            "product",
            |mut cx: StoreContextMut<'_, Host>, (id,): (String,)| {
                Ok((ContextHost::product(cx.data_mut(), id)?,))
            },
        )?;
        context.func_wrap(
            "record",
            |mut cx: StoreContextMut<'_, Host>, (entity, id): (String, String)| {
                Ok((ContextHost::record(cx.data_mut(), entity, id)?,))
            },
        )?;
    }
    let instance = linker.instantiate(&mut *store, component)?;
    let hooks = component
        .get_export_index(None, format!("{namespace}:commerce/hooks@1.0.0"))
        .ok_or_else(|| wasmtime::Error::msg("Missing commerce WIT hooks"))?;
    let evaluate = component
        .get_export_index(Some(&hooks), "evaluate")
        .ok_or_else(|| wasmtime::Error::msg("Missing commerce WIT evaluate"))?;
    instance.get_typed_func(&mut *store, evaluate)
}
#[cfg(test)]
mod tests {
    use super::super::tests::snapshot;
    use super::*;
    #[test]
    fn configured_older_components_keep_snapshots_money_and_resource_guards() {
        let source = include_str!("../../tests/fixtures/snapshot-price.component.wat")
            .replace("veyndo:commerce", "legacy:commerce");
        assert!(crate::test_host::component_with_namespaces(&source, "").is_err());
        let module = crate::test_host::component_with_namespaces(&source, "legacy").unwrap();
        module.validate_interface().unwrap();
        for hook in [Kind::Price, Kind::Shipping, Kind::Discount] {
            assert_eq!(
                module
                    .evaluate(snapshot(25000), hook, "{}")
                    .unwrap()
                    .adjustment
                    .minor,
                250
            );
        }
        assert_eq!(
            module
                .evaluate(snapshot(25000), Kind::Validation, "{}")
                .unwrap()
                .adjustment
                .minor,
            0
        );
        assert!(
            crate::test_host::component_with_namespaces(
                &source.replace("@1.0.0", "@2.0.0"),
                "legacy"
            )
            .is_err()
        );
        for invalid in ["../legacy", "legacy:other", "Legacy", "a,b,c,d,e"] {
            assert!(crate::test_host::component_with_namespaces(&source, invalid).is_err());
        }
        let wrong = source.replace(
            "i32.const 528 i32.const 280 i32.load8_u",
            "i32.const 528 i32.const 3",
        );
        assert!(
            crate::test_host::component_with_namespaces(&wrong, "legacy")
                .unwrap()
                .evaluate(snapshot(25000), Kind::Price, "{}")
                .is_err()
        );
        let looped = source.replace(
            "i32.const 256 call $cart",
            "(loop $again i32.const 256 call $cart br $again)",
        );
        assert!(
            crate::test_host::component_with_namespaces(&looped, "legacy")
                .unwrap()
                .evaluate(snapshot(25000), Kind::Price, "{}")
                .is_err()
        );
    }
}
