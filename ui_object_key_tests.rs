//! Object-expression keys accept API casing without prototype or path injection.

use super::*;
#[test]
fn accepts_api_case_without_prototype_or_path_keys() {
    assert!(object_key("orderId"));
    assert!(object_key("order_id"));
    for bad in [
        "__proto__",
        "constructor",
        "prototype",
        "a.b",
        "a/b",
        "",
        "1key",
    ] {
        assert!(!object_key(bad));
    }
    assert!(!object_key(&"a".repeat(65)));
}
