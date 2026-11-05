;; Keep EUR 100 of the supplied budget unused. Inputs are integer cents.
(module
  (func (export "approve") (param $total i64) (param $limit i64) (result i32)
    local.get $total i64.const 0 i64.ge_s
    local.get $limit i64.const 10000 i64.ge_s i32.and
    local.get $total local.get $limit i64.const 10000 i64.sub i64.le_s i32.and))
