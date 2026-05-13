use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::ast::TypeExpr;
use crate::value::Value;

pub type EnvRef = Rc<RefCell<Environment>>;

#[derive(Debug, Clone)]
pub struct Binding {
    pub value: Value,
    pub type_hint: Option<TypeExpr>,
}

#[derive(Debug)]
pub struct Environment {
    values: BTreeMap<String, Binding>,
    parent: Option<EnvRef>,
}

impl Environment {
    pub fn global() -> EnvRef {
        Rc::new(RefCell::new(Self {
            values: BTreeMap::new(),
            parent: None,
        }))
    }

    pub fn child(parent: EnvRef) -> EnvRef {
        Rc::new(RefCell::new(Self {
            values: BTreeMap::new(),
            parent: Some(parent),
        }))
    }

    pub fn define(env: &EnvRef, name: impl Into<String>, value: Value) {
        Self::define_typed(env, name, value, None);
    }

    pub fn define_typed(
        env: &EnvRef,
        name: impl Into<String>,
        value: Value,
        type_hint: Option<TypeExpr>,
    ) {
        env.borrow_mut().values.insert(
            name.into(),
            Binding {
                value,
                type_hint,
            },
        );
    }

    pub fn get(env: &EnvRef, name: &str) -> Option<Value> {
        Self::get_binding(env, name).map(|binding| binding.value)
    }

    pub fn get_binding(env: &EnvRef, name: &str) -> Option<Binding> {
        let parent = {
            let borrowed = env.borrow();
            if let Some(value) = borrowed.values.get(name) {
                return Some(value.clone());
            }
            borrowed.parent.clone()
        };

        parent.and_then(|parent_env| Self::get_binding(&parent_env, name))
    }

    pub fn assign(env: &EnvRef, name: &str, value: Value) -> bool {
        {
            let mut borrowed = env.borrow_mut();
            if let Some(binding) = borrowed.values.get_mut(name) {
                binding.value = value;
                return true;
            }
        }

        let parent = env.borrow().parent.clone();
        parent
            .as_ref()
            .is_some_and(|parent_env| Self::assign(parent_env, name, value))
    }

    pub fn snapshot_local(env: &EnvRef) -> BTreeMap<String, Value> {
        env.borrow()
            .values
            .iter()
            .map(|(name, binding)| (name.clone(), binding.value.clone()))
            .collect()
    }

    pub fn snapshot_chain(env: &EnvRef) -> Vec<Value> {
        let (mut values, parent) = {
            let borrowed = env.borrow();
            (
                borrowed
                    .values
                    .values()
                    .map(|binding| binding.value.clone())
                    .collect::<Vec<_>>(),
                borrowed.parent.clone(),
            )
        };
        if let Some(parent) = parent {
            values.extend(Self::snapshot_chain(&parent));
        }
        values
    }
}
