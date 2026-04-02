use std::collections::HashMap;
use std::sync::Arc;

use crate::clorure::forms::Symbol;
use crate::clorure::runtime::builtin::{BuiltinError, CallContext};
use crate::clorure::runtime::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Capability {
    FileIo,
    Clock,
    Process,
    Network,
}

pub trait HostAdapter: Send + Sync {
    fn type_name(&self) -> &'static str;
    fn invoke_method(
        &self,
        method: &Symbol,
        args: &[Value],
        context: &CallContext,
    ) -> Result<Value, BuiltinError>;
    fn capabilities(&self) -> Vec<Capability> {
        Vec::new()
    }
}

#[derive(Default)]
pub struct InteropRegistry {
    next_handle: u64,
    adapters: HashMap<u64, Arc<dyn HostAdapter>>,
}

impl InteropRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_adapter(&mut self, adapter: Arc<dyn HostAdapter>) -> Value {
        let handle = self.next_handle;
        self.next_handle += 1;
        self.adapters.insert(handle, adapter);
        Value::OpaqueHandle(handle)
    }

    pub fn invoke(
        &self,
        handle: u64,
        method: &Symbol,
        args: &[Value],
        context: &CallContext,
    ) -> Result<Value, BuiltinError> {
        let adapter = self
            .adapters
            .get(&handle)
            .ok_or_else(|| BuiltinError::new("unknown host handle"))?;
        if !context.capabilities_enabled {
            return Err(BuiltinError::new(
                "host capabilities are disabled in this execution context",
            ));
        }
        adapter.invoke_method(method, args, context)
    }

    pub fn adapter_capabilities(&self, handle: u64) -> Option<Vec<Capability>> {
        self.adapters
            .get(&handle)
            .map(|adapter| adapter.capabilities())
    }
}
