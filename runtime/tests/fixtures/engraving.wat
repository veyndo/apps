;; Engraving-owned rules: printable input is checked by the host; app defines length and fee.
(module
  (func $valid_fee (export "validate_fee") (param $fee i64) (result i32)
    local.get $fee i64.const 0 i64.ge_s
    local.get $fee i64.const 100000 i64.le_s i32.and)
  (func (export "configuration_fee") (param $fee i64) (param $length i64) (result i64)
    local.get $fee call $valid_fee
    local.get $length i64.const 1 i64.ge_s i32.and
    local.get $length i64.const 40 i64.le_s i32.and
    if (result i64) local.get $fee else i64.const -1 end))
