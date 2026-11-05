;; Business orders must reach EUR 50 and stay inside the supplied budget.
(module
  (func (export "approve") (param $total i64) (param $limit i64) (result i32)
    local.get $total i64.const 5000 i64.ge_s
    local.get $total local.get $limit i64.le_s i32.and))
