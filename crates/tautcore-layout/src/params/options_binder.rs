//! Thin binder: typed reads from a free-form [`AttrMap`], with consumed-key tracking.
//!
//! Algorithm modules own field lists, defaults, and presets; this helper only
//! extracts values and reports unknown keys.

use std::collections::BTreeSet;

use tautcore_model::attr::{AttrMap, AttrValue};

/// Warning produced while binding algorithm options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindWarning {
    pub message: String,
}

impl BindWarning {
    pub fn unknown_key(key: impl Into<String>) -> Self {
        Self {
            message: format!("unknown option `{}`", key.into()),
        }
    }

    pub fn message(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

/// Error when a known option has the wrong value shape or an unrecognized atom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindError {
    pub message: String,
}

impl BindError {
    pub fn bad_type(key: &str, expected: &str, value: &AttrValue) -> Self {
        Self {
            message: format!("option `{key}`: expected {expected}, got {value}"),
        }
    }

    pub fn bad_atom(key: &str, value: &str, allowed: &[&str]) -> Self {
        Self {
            message: format!(
                "option `{key}`: unknown value `{value}` (expected one of: {})",
                allowed.join(", ")
            ),
        }
    }
}

/// Tracks which option keys were consumed while binding a typed params struct.
pub struct OptionsBinder<'a> {
    options: &'a AttrMap,
    consumed: BTreeSet<&'a str>,
}

impl<'a> OptionsBinder<'a> {
    pub fn new(options: &'a AttrMap) -> Self {
        Self {
            options,
            consumed: BTreeSet::new(),
        }
    }

    /// Mark `key` consumed if present; return its value.
    pub fn take(&mut self, key: &'static str) -> Option<&'a AttrValue> {
        let value = self.options.get(key)?;
        self.consumed.insert(key);
        Some(value)
    }

    /// First present key among `keys` wins; all looked-up keys that exist are marked consumed.
    pub fn take_any(&mut self, keys: &[&'static str]) -> Option<&'a AttrValue> {
        let mut found = None;
        for &key in keys {
            if let Some(value) = self.options.get(key) {
                self.consumed.insert(key);
                if found.is_none() {
                    found = Some(value);
                }
            }
        }
        found
    }

    pub fn get_f64_any(&mut self, keys: &[&'static str]) -> Result<Option<f64>, BindError> {
        let Some(key) = keys.iter().copied().find(|k| self.options.contains_key(*k)) else {
            return Ok(None);
        };
        match self.take_any(keys) {
            None => Ok(None),
            Some(v) => v
                .as_f64()
                .map(Some)
                .ok_or_else(|| BindError::bad_type(key, "number", v)),
        }
    }

    /// Boolean scalar (`true` / `false` atoms; [`AttrValue::as_bool`]).
    pub fn get_bool(&mut self, key: &'static str) -> Result<Option<bool>, BindError> {
        match self.take(key) {
            None => Ok(None),
            Some(v) => v
                .as_bool()
                .map(Some)
                .ok_or_else(|| BindError::bad_type(key, "boolean", v)),
        }
    }

    /// Atom or string scalar.
    pub fn get_atom(&mut self, key: &'static str) -> Result<Option<&'a str>, BindError> {
        match self.take(key) {
            None => Ok(None),
            Some(v) => v
                .as_str()
                .map(Some)
                .ok_or_else(|| BindError::bad_type(key, "atom or string", v)),
        }
    }

    pub fn get_atom_any(&mut self, keys: &[&'static str]) -> Result<Option<&'a str>, BindError> {
        let Some(key) = keys.iter().copied().find(|k| self.options.contains_key(*k)) else {
            return Ok(None);
        };
        match self.take_any(keys) {
            None => Ok(None),
            Some(v) => v
                .as_str()
                .map(Some)
                .ok_or_else(|| BindError::bad_type(key, "atom or string", v)),
        }
    }

    /// Parse a closed atom set into `T`.
    pub fn get_enum<T: Copy>(
        &mut self,
        key: &'static str,
        mapping: &[(&str, T)],
    ) -> Result<Option<T>, BindError> {
        let Some(raw) = self.get_atom(key)? else {
            return Ok(None);
        };
        for &(name, value) in mapping {
            if raw == name {
                return Ok(Some(value));
            }
        }
        let allowed: Vec<&str> = mapping.iter().map(|(n, _)| *n).collect();
        Err(BindError::bad_atom(key, raw, &allowed))
    }

    pub fn get_enum_any<T: Copy>(
        &mut self,
        keys: &[&'static str],
        mapping: &[(&str, T)],
    ) -> Result<Option<T>, BindError> {
        let Some(key) = keys.iter().copied().find(|k| self.options.contains_key(*k)) else {
            return Ok(None);
        };
        let Some(raw) = self.get_atom_any(keys)? else {
            return Ok(None);
        };
        for &(name, value) in mapping {
            if raw == name {
                return Ok(Some(value));
            }
        }
        let allowed: Vec<&str> = mapping.iter().map(|(n, _)| *n).collect();
        Err(BindError::bad_atom(key, raw, &allowed))
    }

    /// Keys present in the map but never taken.
    pub fn unknown_keys(&self) -> Vec<&'a str> {
        self.options
            .keys()
            .map(|k| k.as_str())
            .filter(|k| !self.consumed.contains(k))
            .collect()
    }

    pub fn unknown_warnings(&self) -> Vec<BindWarning> {
        self.unknown_keys()
            .into_iter()
            .map(BindWarning::unknown_key)
            .collect()
    }
}
