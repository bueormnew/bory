use std::cell::RefCell;
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::fmt;
use std::rc::Rc;
use std::rc::Weak;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use serde_json::Value as JsonValue;

use crate::ast::{FieldDecl, Param, Stmt, TypeExpr, TypeFlavor};
use crate::env::EnvRef;
use crate::error::BoryError;
use crate::runtime::Interpreter;

pub type ListRef = Rc<RefCell<Vec<Value>>>;
pub type ObjectRef = Rc<RefCell<BTreeMap<String, Value>>>;
pub type JobRef = Arc<Mutex<Option<JoinHandle<Result<JsonValue, String>>>>>;
pub type NativeFn = fn(&mut Interpreter, Vec<Value>) -> Result<Value, BoryError>;

thread_local! {
    static HEAP_REGISTRY: RefCell<HeapRegistry> = RefCell::new(HeapRegistry::default());
}

#[derive(Clone)]
pub enum Value {
    Nil,
    Bool(bool),
    Number(f64),
    String(String),
    List(ListRef),
    Object(ObjectRef),
    Type(Rc<TypeDef>),
    Function(Rc<UserFunction>),
    NativeFunction(Rc<NativeFunction>),
    Job(JobRef),
}

#[derive(Clone)]
pub struct UserFunction {
    pub name: String,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Vec<Stmt>,
    pub closure: EnvRef,
}

#[derive(Clone)]
pub struct NativeFunction {
    pub name: String,
    pub min_arity: usize,
    pub max_arity: Option<usize>,
    pub func: NativeFn,
}

#[derive(Clone)]
pub struct TypeDef {
    pub flavor: TypeFlavor,
    pub name: String,
    pub fields: Vec<FieldDecl>,
    pub body: Vec<Stmt>,
    pub closure: EnvRef,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HeapStats {
    pub allocations: usize,
    pub tracked_lists: usize,
    pub tracked_objects: usize,
    pub active_lists: usize,
    pub active_objects: usize,
    pub reclaimed_entries: usize,
    pub sweeps: usize,
    pub minor_sweeps: usize,
    pub major_sweeps: usize,
    pub promoted_entries: usize,
    pub compacted_entries: usize,
    pub gen0_entries: usize,
    pub gen1_entries: usize,
    pub gen2_entries: usize,
}

#[derive(Default)]
struct HeapRegistry {
    next_id: u64,
    allocations: usize,
    lists: Vec<ListEntry>,
    objects: Vec<ObjectEntry>,
    reclaimed_entries: usize,
    sweeps: usize,
    minor_sweeps: usize,
    major_sweeps: usize,
    promoted_entries: usize,
    compacted_entries: usize,
}

#[derive(Clone)]
struct ListEntry {
    id: u64,
    weak: Weak<RefCell<Vec<Value>>>,
    generation: u8,
    survived: usize,
}

#[derive(Clone)]
struct ObjectEntry {
    id: u64,
    weak: Weak<RefCell<BTreeMap<String, Value>>>,
    generation: u8,
    survived: usize,
}

impl NativeFunction {
    pub fn new(
        name: impl Into<String>,
        min_arity: usize,
        max_arity: Option<usize>,
        func: NativeFn,
    ) -> Self {
        Self {
            name: name.into(),
            min_arity,
            max_arity,
            func,
        }
    }
}

impl UserFunction {
    pub fn new(
        name: impl Into<String>,
        params: Vec<Param>,
        return_type: Option<TypeExpr>,
        body: Vec<Stmt>,
        closure: EnvRef,
    ) -> Self {
        Self {
            name: name.into(),
            params,
            return_type,
            body,
            closure,
        }
    }
}

impl TypeDef {
    pub fn new(
        flavor: TypeFlavor,
        name: impl Into<String>,
        fields: Vec<FieldDecl>,
        body: Vec<Stmt>,
        closure: EnvRef,
    ) -> Self {
        Self {
            flavor,
            name: name.into(),
            fields,
            body,
            closure,
        }
    }

    pub fn flavor_name(&self) -> &'static str {
        match self.flavor {
            TypeFlavor::Struct => "struct",
            TypeFlavor::Class => "class",
        }
    }
}

impl Value {
    pub fn list(values: Vec<Value>) -> Self {
        let value = Rc::new(RefCell::new(values));
        HEAP_REGISTRY.with(|registry| registry.borrow_mut().register_list(&value));
        Self::List(value)
    }

    pub fn object(values: BTreeMap<String, Value>) -> Self {
        let value = Rc::new(RefCell::new(values));
        HEAP_REGISTRY.with(|registry| registry.borrow_mut().register_object(&value));
        Self::Object(value)
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Nil => "nil",
            Self::Bool(_) => "bool",
            Self::Number(_) => "number",
            Self::String(_) => "text",
            Self::List(_) => "list",
            Self::Object(_) => "object",
            Self::Type(typedef) => typedef.flavor_name(),
            Self::Function(_) => "task",
            Self::NativeFunction(_) => "native-task",
            Self::Job(_) => "job",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Nil => false,
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
            Self::List(values) => !values.borrow().is_empty(),
            Self::Object(values) => !values.borrow().is_empty(),
            Self::Type(_) | Self::Function(_) | Self::NativeFunction(_) | Self::Job(_) => true,
        }
    }

    pub fn deep_copy(&self) -> Self {
        match self {
            Self::Nil => Self::Nil,
            Self::Bool(value) => Self::Bool(*value),
            Self::Number(value) => Self::Number(*value),
            Self::String(value) => Self::String(value.clone()),
            Self::List(values) => Self::list(values.borrow().iter().map(Value::deep_copy).collect()),
            Self::Object(values) => {
                let copied = values
                    .borrow()
                    .iter()
                    .map(|(key, value)| (key.clone(), value.deep_copy()))
                    .collect();
                Self::object(copied)
            }
            Self::Type(typedef) => Self::Type(typedef.clone()),
            Self::Function(function) => Self::Function(function.clone()),
            Self::NativeFunction(function) => Self::NativeFunction(function.clone()),
            Self::Job(job) => Self::Job(job.clone()),
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            Self::Number(value) if value.fract() == 0.0 => Some(*value as i64),
            _ => None,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nil => write!(f, "nil"),
            Self::Bool(true) => write!(f, "yes"),
            Self::Bool(false) => write!(f, "no"),
            Self::Number(value) => write!(f, "{}", format_number(*value)),
            Self::String(value) => write!(f, "{value}"),
            Self::List(values) => {
                let rendered = values
                    .borrow()
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "[{rendered}]")
            }
            Self::Object(values) => {
                let rendered = values
                    .borrow()
                    .iter()
                    .map(|(key, value)| format!("{key}: {value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{{{rendered}}}")
            }
            Self::Type(typedef) => write!(f, "<{} {}>", typedef.flavor_name(), typedef.name),
            Self::Function(function) => write!(f, "<task {}>", function.name),
            Self::NativeFunction(function) => write!(f, "<native {}>", function.name),
            Self::Job(_) => write!(f, "<job>"),
        }
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl fmt::Debug for UserFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<task {}>", self.name)
    }
}

impl fmt::Debug for NativeFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<native {}>", self.name)
    }
}

impl fmt::Debug for TypeDef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{} {}>", self.flavor_name(), self.name)
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        values_equal(self, other)
    }
}

pub fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Nil, Value::Nil) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => {
            if a.is_nan() || b.is_nan() {
                false
            } else {
                (*a - *b).abs() < f64::EPSILON
            }
        }
        (Value::String(a), Value::String(b)) => a == b,
        (Value::List(a), Value::List(b)) => {
            let a = a.borrow();
            let b = b.borrow();
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| values_equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            let a = a.borrow();
            let b = b.borrow();
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| values_equal(value, other)))
        }
        (Value::Function(a), Value::Function(b)) => Rc::ptr_eq(a, b),
        (Value::NativeFunction(a), Value::NativeFunction(b)) => Rc::ptr_eq(a, b),
        (Value::Type(a), Value::Type(b)) => Rc::ptr_eq(a, b),
        (Value::Job(a), Value::Job(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}

pub fn format_number(value: f64) -> String {
    if value.is_nan() || value.is_infinite() {
        return value.to_string();
    }

    let mut text = format!("{value}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

pub fn heap_stats() -> HeapStats {
    HEAP_REGISTRY.with(|registry| registry.borrow().stats())
}

pub fn collect_heap() -> HeapStats {
    HEAP_REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        registry.collect_minor(&[]);
        registry.stats()
    })
}

pub fn collect_heap_with_roots(roots: &[Value], major: bool) -> HeapStats {
    HEAP_REGISTRY.with(|registry| {
        let mut registry = registry.borrow_mut();
        if major {
            registry.collect_major(roots);
        } else {
            registry.collect_minor(roots);
        }
        registry.stats()
    })
}

impl HeapRegistry {
    fn register_list(&mut self, value: &ListRef) {
        self.next_id += 1;
        self.allocations += 1;
        self.lists.push(ListEntry {
            id: self.next_id,
            weak: Rc::downgrade(value),
            generation: 0,
            survived: 0,
        });
    }

    fn register_object(&mut self, value: &ObjectRef) {
        self.next_id += 1;
        self.allocations += 1;
        self.objects.push(ObjectEntry {
            id: self.next_id,
            weak: Rc::downgrade(value),
            generation: 0,
            survived: 0,
        });
    }

    fn collect_minor(&mut self, roots: &[Value]) {
        self.sweeps += 1;
        self.minor_sweeps += 1;
        self.collect_internal(roots, false);
    }

    fn collect_major(&mut self, roots: &[Value]) {
        self.sweeps += 1;
        self.major_sweeps += 1;
        self.collect_internal(roots, true);
    }

    fn collect_internal(&mut self, roots: &[Value], major: bool) {
        let mut reachable_lists = HashSet::new();
        let mut reachable_objects = HashSet::new();
        let mut seen_lists = HashSet::new();
        let mut seen_objects = HashSet::new();

        for root in roots {
            mark_value(
                root,
                &mut reachable_lists,
                &mut reachable_objects,
                &mut seen_lists,
                &mut seen_objects,
            );
        }

        let before = self.lists.len() + self.objects.len();
        let mut compacted_now = 0usize;
        let mut promoted_now = 0usize;

        self.lists.retain_mut(|entry| {
            let Some(list) = entry.weak.upgrade() else {
                return false;
            };
            let addr = Rc::as_ptr(&list) as usize;
            let should_keep = reachable_lists.contains(&addr) || list.borrow().len() > 0;
            let in_scope = major || entry.generation == 0;
            if should_keep {
                entry.survived += 1;
                if entry.generation < 2 && entry.survived >= 2 {
                    entry.generation += 1;
                    entry.survived = 0;
                    promoted_now += 1;
                }
                compact_list(&list);
                compacted_now += 1;
                true
            } else if in_scope {
                list.borrow_mut().clear();
                false
            } else {
                true
            }
        });

        self.objects.retain_mut(|entry| {
            let Some(object) = entry.weak.upgrade() else {
                return false;
            };
            let addr = Rc::as_ptr(&object) as usize;
            let should_keep = reachable_objects.contains(&addr) || !object.borrow().is_empty();
            let in_scope = major || entry.generation == 0;
            if should_keep {
                entry.survived += 1;
                if entry.generation < 2 && entry.survived >= 2 {
                    entry.generation += 1;
                    entry.survived = 0;
                    promoted_now += 1;
                }
                compact_object(&object);
                compacted_now += 1;
                true
            } else if in_scope {
                object.borrow_mut().clear();
                false
            } else {
                true
            }
        });

        let after = self.lists.len() + self.objects.len();
        self.reclaimed_entries += before.saturating_sub(after);
        self.promoted_entries += promoted_now;
        self.compacted_entries += compacted_now;
    }

    fn stats(&self) -> HeapStats {
        let gen0_entries = self
            .lists
            .iter()
            .filter(|entry| entry.generation == 0)
            .count()
            + self
                .objects
                .iter()
                .filter(|entry| entry.generation == 0)
                .count();
        let gen1_entries = self
            .lists
            .iter()
            .filter(|entry| entry.generation == 1)
            .count()
            + self
                .objects
                .iter()
                .filter(|entry| entry.generation == 1)
                .count();
        let gen2_entries = self
            .lists
            .iter()
            .filter(|entry| entry.generation >= 2)
            .count()
            + self
                .objects
                .iter()
                .filter(|entry| entry.generation >= 2)
                .count();
        HeapStats {
            allocations: self.allocations,
            tracked_lists: self.lists.len(),
            tracked_objects: self.objects.len(),
            active_lists: self
                .lists
                .iter()
                .filter(|entry| entry.weak.strong_count() > 0)
                .count(),
            active_objects: self
                .objects
                .iter()
                .filter(|entry| entry.weak.strong_count() > 0)
                .count(),
            reclaimed_entries: self.reclaimed_entries,
            sweeps: self.sweeps,
            minor_sweeps: self.minor_sweeps,
            major_sweeps: self.major_sweeps,
            promoted_entries: self.promoted_entries,
            compacted_entries: self.compacted_entries,
            gen0_entries,
            gen1_entries,
            gen2_entries,
        }
    }
}

fn mark_value(
    value: &Value,
    reachable_lists: &mut HashSet<usize>,
    reachable_objects: &mut HashSet<usize>,
    seen_lists: &mut HashSet<usize>,
    seen_objects: &mut HashSet<usize>,
) {
    match value {
        Value::List(list) => {
            let addr = Rc::as_ptr(list) as usize;
            if !seen_lists.insert(addr) {
                return;
            }
            reachable_lists.insert(addr);
            let items = list.borrow().clone();
            for item in items {
                mark_value(&item, reachable_lists, reachable_objects, seen_lists, seen_objects);
            }
        }
        Value::Object(object) => {
            let addr = Rc::as_ptr(object) as usize;
            if !seen_objects.insert(addr) {
                return;
            }
            reachable_objects.insert(addr);
            let values = object.borrow().values().cloned().collect::<Vec<_>>();
            for value in values {
                mark_value(&value, reachable_lists, reachable_objects, seen_lists, seen_objects);
            }
        }
        _ => {}
    }
}

fn compact_list(list: &ListRef) {
    let cloned = {
        let borrowed = list.borrow();
        let mut compacted = borrowed.clone();
        compacted.shrink_to_fit();
        compacted
    };
    *list.borrow_mut() = cloned;
}

fn compact_object(object: &ObjectRef) {
    let compacted = object
        .borrow()
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    *object.borrow_mut() = compacted;
}
