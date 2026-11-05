;; Limit any individual business purchase to EUR 250, within its budget.
(module
  (func (export "approve") (param $total i64) (param $limit i64) (result i32)
    local.get $total i64.const 0 i64.ge_s
    local.get $total i64.const 25000 i64.le_s i32.and
    local.get $total local.get $limit i64.le_s i32.and))
