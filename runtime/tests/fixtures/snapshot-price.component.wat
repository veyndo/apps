;; Typed fixture: calls the real host cart snapshot and returns one percent of its subtotal.
(component
  (import "veyndo:commerce/context@1.0.0" (instance $context
    (type $money-local (record (field "minor" s64) (field "currency" string) (field "scale" u8)))
    (export "money" (type $money (eq $money-local)))
    (type $item-local (record (field "id" string) (field "quantity" u32) (field "price" $money)))
    (export "cart-item" (type $item (eq $item-local)))
    (type $items (list $item))
    (type $cart-local (record (field "items" $items) (field "subtotal" $money)))
    (export "cart-snapshot" (type $cart (eq $cart-local)))
    (type $product-local (record (field "id" string) (field "stock" u32) (field "price" $money)))
    (export "product-snapshot" (type $product (eq $product-local)))
    (type $maybe-product (option $product))
    (type $record-local (record (field "id" string) (field "revision" u64) (field "fields-json" string)))
    (export "app-record" (type $record (eq $record-local)))
    (type $maybe-record (option $record))
    (export "cart" (func (result $cart)))
    (export "product" (func (param "id" string) (result $maybe-product)))
    (export "record" (func (param "entity" string) (param "id" string) (result $maybe-record)))
  ))
  (alias export $context "money" (type $money))
  (core module $memory
    (memory (export "memory") 1 16)
    (global $heap (mut i32) (i32.const 4096))
    (func (export "realloc") (param i32 i32 i32 i32) (result i32)
      (local $p i32)
      global.get $heap local.set $p
      global.get $heap local.get 3 i32.add i32.const 7 i32.add i32.const -8 i32.and global.set $heap
      local.get $p)
  )
  (core instance $mem (instantiate $memory))
  (alias core export $mem "memory" (core memory $mem))
  (alias core export $mem "realloc" (core func $alloc))
  (alias export $context "cart" (func $cart-func))
  (core func $read-cart (canon lower (func $cart-func) (memory $mem) (realloc $alloc)))
  (core instance $host (export "cart" (func $read-cart)) (export "memory" (memory $mem)))
  (core module $guest
    (import "host" "cart" (func $cart (param i32)))
    (import "host" "memory" (memory 1 16))
    (data (i32.const 64) "snapshot_price")
    (func (export "evaluate") (param $kind i32) (param i32 i32) (result i32)
      i32.const 256 call $cart
      ;; Money begins after the eight-byte items-list descriptor.
      i32.const 512 i32.const 264 i64.load i64.const 100 i64.div_s i64.store
      ;; Validation is deliberately a zero-money outcome.
      local.get $kind i32.const 3 i32.eq if i32.const 512 i64.const 0 i64.store end
      i32.const 520 i32.const 272 i32.load i32.store
      i32.const 524 i32.const 276 i32.load i32.store
      i32.const 528 i32.const 280 i32.load8_u i32.store8
      i32.const 536 i32.const 1 i32.store8
      i32.const 540 i32.const 64 i32.store
      i32.const 544 i32.const 14 i32.store
      i32.const 512)
  )
  (core instance $code (instantiate $guest (with "host" (instance $host))))
  (type $kind (enum "price" "shipping" "discount" "validation"))
  (type $outcome (record (field "adjustment" $money) (field "accepted" bool) (field "reason-code" string)))
  (func $evaluate (param "hook" $kind) (param "input" string) (result $outcome)
    (canon lift (core func $code "evaluate") (memory $mem) (realloc $alloc)))
  (instance $hooks
    (export "kind" (type $kind))
    (export "outcome" (type $outcome))
    (export "evaluate" (func $evaluate)))
  (export "veyndo:commerce/hooks@1.0.0" (instance $hooks))
)
