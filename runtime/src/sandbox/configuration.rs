//! Bounded integer configuration arguments share the existing no-import Wasmtime engine and resource budgets.
use super::*;
impl Sandbox {
    pub(super) fn core_instance(&self) -> Result<(Store<Limits>, Instance), String> {
        let mut store = Store::new(
            &self.engine,
            Limits {
                memory: StoreLimitsBuilder::new()
                    .memory_size(MEMORY_BYTES)
                    .table_elements(TABLE_ELEMENTS)
                    .instances(1)
                    .memories(1)
                    .tables(1)
                    .build(),
            },
        );
        store.limiter(|s| &mut s.memory);
        store.set_epoch_deadline(10);
        store.set_fuel(10_000).map_err(|e| e.to_string())?;
        let instance = Instance::new(
            &mut store,
            self.module.as_ref().ok_or("Core ABI required")?,
            &[],
        )
        .map_err(|e| e.to_string())?;
        Ok((store, instance))
    }
    fn configuration_function(
        &self,
        store: &mut Store<Limits>,
        instance: &Instance,
        arity: usize,
    ) -> Result<wasmtime::Func, String> {
        if !(1..=8).contains(&arity) {
            return Err("Configuration argument budget exceeded".into());
        }
        let f = instance
            .get_func(&mut *store, "configuration_fee")
            .ok_or("Missing configuration function")?;
        let ty = f.ty(&*store);
        if ty.params().count() != arity + 1
            || ty.params().any(|t| !matches!(t, wasmtime::ValType::I64))
            || ty.results().count() != 1
            || ty.results().any(|t| !matches!(t, wasmtime::ValType::I64))
        {
            return Err("Configuration function must accept declared integer arguments and return one integer".into());
        }
        Ok(f)
    }
    pub fn contribution_args(&self, fee_minor: i64, args: &[i64]) -> Result<i64, String> {
        let (mut store, instance) = self.core_instance()?;
        let f = self.configuration_function(&mut store, &instance, args.len())?;
        let values = std::iter::once(fee_minor)
            .chain(args.iter().copied())
            .map(wasmtime::Val::I64)
            .collect::<Vec<_>>();
        let mut out = [wasmtime::Val::I64(0)];
        f.call(&mut store, &values, &mut out)
            .map_err(|e| format!("{e:#}"))?;
        out[0].i64().ok_or("Invalid configuration result".into())
    }
    pub fn validate_contribution_args(&self, fee_minor: i64, arity: usize) -> Result<bool, String> {
        let (mut store, instance) = self.core_instance()?;
        self.configuration_function(&mut store, &instance, arity)?;
        instance
            .get_typed_func::<i64, i32>(&mut store, "validate_fee")
            .map_err(|e| e.to_string())?
            .call(&mut store, fee_minor)
            .map(|v| v == 1)
            .map_err(|e| format!("{e:#}"))
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn declared_arguments_execute_real_guest_and_reject_wrong_abi_or_budget() {
        let guest=crate::test_host::sandbox("(module (func (export \"validate_fee\") (param i64) (result i32) i32.const 1) (func (export \"configuration_fee\") (param $fee i64) (param $width i64) (param $depth i64) (result i64) local.get $fee local.get $width i64.add local.get $depth i64.add))").unwrap();
        assert!(guest.validate_contribution_args(100, 2).unwrap());
        assert_eq!(guest.contribution_args(100, &[240, 80]).unwrap(), 420);
        assert_eq!(guest.contribution_args(100, &[260, 80]).unwrap(), 440);
        assert!(guest.contribution_args(100, &[1]).is_err());
        assert!(guest.contribution_args(100, &[]).is_err());
        assert!(guest.contribution_args(100, &[0; 9]).is_err());
        let trap=crate::test_host::sandbox("(module (func (export \"configuration_fee\") (param i64 i64) (result i64) (loop br 0) i64.const 0))").unwrap();
        assert!(
            trap.contribution_args(1, &[1])
                .unwrap_err()
                .contains("fuel")
        );
    }
}
