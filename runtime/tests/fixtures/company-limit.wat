(module
  (func (export "approve") (param $total i64) (param $limit i64) (result i32)
    local.get $total
    local.get $limit
    i64.le_s))
