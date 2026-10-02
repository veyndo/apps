//! Explicit read adapter for persisted v0.5 engraving carts; completed order snapshots remain unchanged.
use super::*;
pub(crate) fn upgrade_cart(data: &mut Value) {
    if let Some(values) = data["app_configurations"].as_object_mut() {
        for (product, value) in values {
            if value.get("app").is_none() {
                value["app"] = json!("engraving");
                value["productId"] = json!(product);
                value["fields"] = json!({"text":value["text"]});
                value["entity"] = json!("rules");
                value["record"] = json!("default");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_cart_adapter_is_explicit_and_idempotent() {
        let mut cart = json!({"app_configurations":{"mug":{"text":"Ada","feeMinor":300,"ruleRevision":1,"appVersion":"1.0.0"}},"order":{"immutable":"snapshot"}});
        upgrade_cart(&mut cart);
        assert_eq!(
            cart["app_configurations"]["mug"]["fields"],
            json!({"text":"Ada"})
        );
        let snapshot = cart.clone();
        upgrade_cart(&mut cart);
        assert_eq!(cart, snapshot);
        assert_eq!(cart["order"], json!({"immutable":"snapshot"}));
    }
}
