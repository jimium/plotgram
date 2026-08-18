//! Shared algorithm-option binding helpers.
//!
//! DSL / [`LayoutContract`](tautcore_model::contract::LayoutContract) keep free-form
//! [`AttrMap`](tautcore_model::attr::AttrMap) options; each layout/router module binds
//! them into a typed params struct at its entry point.

mod options_binder;

pub use options_binder::{BindError, BindWarning, OptionsBinder};
