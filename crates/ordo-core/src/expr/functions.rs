//! Built-in functions
//!
//! Provides built-in functions for expressions.
//!
//! # Performance Optimization
//!
//! The default built-in functions are stored in a global singleton (`GLOBAL_BUILTIN_REGISTRY`)
//! to avoid repeated registration overhead. Custom functions can still be added per-registry.

use crate::context::Value;
use crate::error::{OrdoError, Result};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::{Arc, OnceLock};

/// Function signature type
pub type FunctionFn = Arc<dyn Fn(&[Value]) -> Result<Value> + Send + Sync>;

/// Global singleton for built-in function registry (shared across all evaluators)
static GLOBAL_BUILTIN_REGISTRY: OnceLock<Arc<FunctionRegistry>> = OnceLock::new();

/// Per-thread regex cache to avoid recompiling the same pattern repeatedly.
thread_local! {
    static REGEX_CACHE: RefCell<lru::LruCache<String, regex::Regex>> =
        RefCell::new(lru::LruCache::new(NonZeroUsize::new(64).unwrap()));
}

/// Compile or retrieve a cached regex for the given pattern.
fn get_or_compile_regex(pattern: &str) -> Result<regex::Regex> {
    REGEX_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(re) = cache.get(pattern) {
            return Ok(re.clone());
        }
        let re = regex::Regex::new(pattern)
            .map_err(|e| OrdoError::eval_error(format!("Invalid regex pattern: {e}")))?;
        cache.put(pattern.to_string(), re.clone());
        Ok(re)
    })
}

/// Get the global built-in function registry (lazily initialized)
#[inline]
pub fn global_builtin_registry() -> &'static Arc<FunctionRegistry> {
    GLOBAL_BUILTIN_REGISTRY.get_or_init(|| {
        let mut registry = FunctionRegistry {
            functions: HashMap::new(),
            custom_only: false,
        };
        registry.register_builtins();
        Arc::new(registry)
    })
}

/// Function registry
pub struct FunctionRegistry {
    functions: HashMap<String, FunctionFn>,
    /// If true, this registry only contains custom functions (uses global for builtins)
    custom_only: bool,
}

impl Default for FunctionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FunctionRegistry {
    /// Create a new function registry.
    ///
    /// This creates a lightweight registry that delegates built-in function
    /// lookups to the global singleton, avoiding repeated registration overhead.
    pub fn new() -> Self {
        Self {
            functions: HashMap::new(),
            custom_only: true, // Use global registry for builtins
        }
    }

    /// Create a new standalone registry with all built-in functions registered locally.
    /// Use this if you need to modify built-in function behavior.
    pub fn new_standalone() -> Self {
        let mut registry = Self {
            functions: HashMap::new(),
            custom_only: false,
        };
        registry.register_builtins();
        registry
    }

    /// Register built-in functions
    fn register_builtins(&mut self) {
        // ================================================================
        // String functions (original)
        // ================================================================

        self.register("len", |args| {
            require_args("len", args, 1)?;
            match &args[0] {
                Value::String(s) => Ok(Value::int(s.len() as i64)),
                Value::Array(a) => Ok(Value::int(a.len() as i64)),
                Value::Object(o) => Ok(Value::int(o.len() as i64)),
                v => Err(OrdoError::type_error(
                    "string, array, or object",
                    v.type_name(),
                )),
            }
        });

        self.register("upper", |args| {
            require_args("upper", args, 1)?;
            let s = require_string("upper", &args[0])?;
            Ok(Value::string(s.to_uppercase()))
        });

        self.register("lower", |args| {
            require_args("lower", args, 1)?;
            let s = require_string("lower", &args[0])?;
            Ok(Value::string(s.to_lowercase()))
        });

        self.register("trim", |args| {
            require_args("trim", args, 1)?;
            let s = require_string("trim", &args[0])?;
            Ok(Value::string(s.trim()))
        });

        self.register("starts_with", |args| {
            require_args("starts_with", args, 2)?;
            let s = require_string("starts_with", &args[0])?;
            let prefix = require_string("starts_with", &args[1])?;
            Ok(Value::bool(s.starts_with(prefix)))
        });

        self.register("ends_with", |args| {
            require_args("ends_with", args, 2)?;
            let s = require_string("ends_with", &args[0])?;
            let suffix = require_string("ends_with", &args[1])?;
            Ok(Value::bool(s.ends_with(suffix)))
        });

        self.register("contains_str", |args| {
            require_args("contains_str", args, 2)?;
            let s = require_string("contains_str", &args[0])?;
            let sub = require_string("contains_str", &args[1])?;
            Ok(Value::bool(s.contains(sub)))
        });

        self.register("substring", |args| {
            if args.len() < 2 || args.len() > 3 {
                return Err(OrdoError::FunctionArgError {
                    name: "substring".into(),
                    message: "expected 2 or 3 arguments".into(),
                });
            }
            let s = require_string("substring", &args[0])?;
            let start = require_int("substring", &args[1])? as usize;
            let end = if args.len() == 3 {
                require_int("substring", &args[2])? as usize
            } else {
                s.len()
            };
            let result: String = s.chars().skip(start).take(end.saturating_sub(start)).collect();
            Ok(Value::string(result))
        });

        // ================================================================
        // String functions (OPA-aligned additions)
        // ================================================================

        // split(str, delimiter) -> Array  — OPA: strings.split
        self.register("split", |args| {
            require_args("split", args, 2)?;
            let s = require_string("split", &args[0])?;
            let delim = require_string("split", &args[1])?;
            let parts: Vec<Value> = s.split(delim).map(Value::string).collect();
            Ok(Value::array(parts))
        });

        // join(array, delimiter) -> String  — OPA: strings.join
        self.register("join", |args| {
            require_args("join", args, 2)?;
            let arr = require_array("join", &args[0])?;
            let delim = require_string("join", &args[1])?;
            let parts: Result<Vec<&str>> = arr
                .iter()
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| OrdoError::type_error("string", v.type_name()))
                })
                .collect();
            Ok(Value::string(parts?.join(delim)))
        });

        // replace(str, from, to) -> String  — OPA: strings.replace_n
        self.register("replace", |args| {
            require_args("replace", args, 3)?;
            let s = require_string("replace", &args[0])?;
            let from = require_string("replace", &args[1])?;
            let to = require_string("replace", &args[2])?;
            Ok(Value::string(s.replace(from, to)))
        });

        // trim_start(str) -> String
        self.register("trim_start", |args| {
            require_args("trim_start", args, 1)?;
            let s = require_string("trim_start", &args[0])?;
            Ok(Value::string(s.trim_start()))
        });

        // trim_end(str) -> String
        self.register("trim_end", |args| {
            require_args("trim_end", args, 1)?;
            let s = require_string("trim_end", &args[0])?;
            Ok(Value::string(s.trim_end()))
        });

        // index_of(str, substring) -> Int  (-1 if not found)  — OPA: indexof
        self.register("index_of", |args| {
            require_args("index_of", args, 2)?;
            let s = require_string("index_of", &args[0])?;
            let sub = require_string("index_of", &args[1])?;
            let idx = s
                .find(sub)
                .map(|byte_pos| s[..byte_pos].chars().count() as i64)
                .unwrap_or(-1);
            Ok(Value::int(idx))
        });

        // repeat(str, n) -> String
        self.register("repeat", |args| {
            require_args("repeat", args, 2)?;
            let s = require_string("repeat", &args[0])?;
            let n = require_int("repeat", &args[1])?;
            if n < 0 {
                return Err(OrdoError::eval_error("repeat count must be non-negative"));
            }
            Ok(Value::string(s.repeat(n as usize)))
        });

        // pad_left(str, total_len, pad_char) -> String
        self.register("pad_left", |args| {
            require_args("pad_left", args, 3)?;
            let s = require_string("pad_left", &args[0])?;
            let total = require_int("pad_left", &args[1])?.max(0) as usize;
            let pad_ch = require_string("pad_left", &args[2])?;
            let pad_char = pad_ch.chars().next().unwrap_or(' ');
            let char_count = s.chars().count();
            if char_count >= total {
                return Ok(Value::string(s));
            }
            let pad_count = total - char_count;
            let mut result = std::iter::repeat(pad_char)
                .take(pad_count)
                .collect::<String>();
            result.push_str(s);
            Ok(Value::string(result))
        });

        // ================================================================
        // Regex functions (OPA-aligned)  — regex crate in workspace
        // ================================================================

        // regex_match(pattern, str) -> Bool  — OPA: regex.match
        self.register("regex_match", |args| {
            require_args("regex_match", args, 2)?;
            let pattern = require_string("regex_match", &args[0])?;
            let s = require_string("regex_match", &args[1])?;
            let re = get_or_compile_regex(pattern)?;
            Ok(Value::bool(re.is_match(s)))
        });

        // regex_find(pattern, str) -> String | Null  — OPA: regex.find_n
        self.register("regex_find", |args| {
            require_args("regex_find", args, 2)?;
            let pattern = require_string("regex_find", &args[0])?;
            let s = require_string("regex_find", &args[1])?;
            let re = get_or_compile_regex(pattern)?;
            Ok(re
                .find(s)
                .map(|m| Value::string(m.as_str()))
                .unwrap_or(Value::Null))
        });

        // regex_replace(str, pattern, replacement) -> String  — OPA: regex.replace_n
        self.register("regex_replace", |args| {
            require_args("regex_replace", args, 3)?;
            let s = require_string("regex_replace", &args[0])?;
            let pattern = require_string("regex_replace", &args[1])?;
            let replacement = require_string("regex_replace", &args[2])?;
            let re = get_or_compile_regex(pattern)?;
            Ok(Value::string(re.replace_all(s, replacement).as_ref()))
        });

        // regex_split(str, pattern) -> Array  — OPA: regex.split
        self.register("regex_split", |args| {
            require_args("regex_split", args, 2)?;
            let s = require_string("regex_split", &args[0])?;
            let pattern = require_string("regex_split", &args[1])?;
            let re = get_or_compile_regex(pattern)?;
            let parts: Vec<Value> = re.split(s).map(Value::string).collect();
            Ok(Value::array(parts))
        });

        // ================================================================
        // Math functions (original)
        // ================================================================

        self.register("abs", |args| {
            require_args("abs", args, 1)?;
            match &args[0] {
                Value::Int(n) => n
                    .checked_abs()
                    .map(Value::int)
                    .ok_or_else(|| OrdoError::eval_error("Integer overflow in abs()")),
                Value::Float(n) => Ok(Value::float(n.abs())),
                v => Err(OrdoError::type_error("number", v.type_name())),
            }
        });

        self.register("min", |args| {
            if args.is_empty() {
                return Err(OrdoError::FunctionArgError {
                    name: "min".into(),
                    message: "expected at least 1 argument".into(),
                });
            }
            let mut result = &args[0];
            for arg in &args[1..] {
                if arg.compare(result) == Some(std::cmp::Ordering::Less) {
                    result = arg;
                }
            }
            Ok(result.clone())
        });

        self.register("max", |args| {
            if args.is_empty() {
                return Err(OrdoError::FunctionArgError {
                    name: "max".into(),
                    message: "expected at least 1 argument".into(),
                });
            }
            let mut result = &args[0];
            for arg in &args[1..] {
                if arg.compare(result) == Some(std::cmp::Ordering::Greater) {
                    result = arg;
                }
            }
            Ok(result.clone())
        });

        self.register("floor", |args| {
            require_args("floor", args, 1)?;
            let n = require_float("floor", &args[0])?;
            Ok(Value::int(n.floor() as i64))
        });

        self.register("ceil", |args| {
            require_args("ceil", args, 1)?;
            let n = require_float("ceil", &args[0])?;
            Ok(Value::int(n.ceil() as i64))
        });

        self.register("round", |args| {
            require_args("round", args, 1)?;
            let n = require_float("round", &args[0])?;
            Ok(Value::int(n.round() as i64))
        });

        // ================================================================
        // Math functions (OPA-aligned additions)
        // ================================================================

        // pow(base, exp) -> Float  — OPA: numbers.range / math.pow
        self.register("pow", |args| {
            require_args("pow", args, 2)?;
            let base = require_float("pow", &args[0])?;
            let exp = require_float("pow", &args[1])?;
            Ok(Value::float(base.powf(exp)))
        });

        // sqrt(n) -> Float
        self.register("sqrt", |args| {
            require_args("sqrt", args, 1)?;
            let n = require_float("sqrt", &args[0])?;
            if n < 0.0 {
                return Err(OrdoError::eval_error(
                    "sqrt of negative number is undefined",
                ));
            }
            Ok(Value::float(n.sqrt()))
        });

        // log(n) -> Float  (natural logarithm)
        self.register("log", |args| {
            require_args("log", args, 1)?;
            let n = require_float("log", &args[0])?;
            if n <= 0.0 {
                return Err(OrdoError::eval_error(
                    "log of non-positive number is undefined",
                ));
            }
            Ok(Value::float(n.ln()))
        });

        // ================================================================
        // Array functions (original)
        // ================================================================

        self.register("sum", |args| {
            require_args("sum", args, 1)?;
            let arr = require_array("sum", &args[0])?;
            let mut int_sum: i64 = 0;
            let mut float_sum: f64 = 0.0;
            let mut has_float = false;

            for v in arr {
                match v {
                    Value::Int(n) => int_sum += n,
                    Value::Float(n) => {
                        has_float = true;
                        float_sum += n;
                    }
                    _ => return Err(OrdoError::type_error("number", v.type_name())),
                }
            }

            if has_float {
                Ok(Value::float(int_sum as f64 + float_sum))
            } else {
                Ok(Value::int(int_sum))
            }
        });

        self.register("avg", |args| {
            require_args("avg", args, 1)?;
            let arr = require_array("avg", &args[0])?;
            if arr.is_empty() {
                return Ok(Value::float(0.0));
            }

            let mut sum: f64 = 0.0;
            for v in arr {
                match v {
                    Value::Int(n) => sum += *n as f64,
                    Value::Float(n) => sum += n,
                    _ => return Err(OrdoError::type_error("number", v.type_name())),
                }
            }

            Ok(Value::float(sum / arr.len() as f64))
        });

        self.register("count", |args| {
            require_args("count", args, 1)?;
            let arr = require_array("count", &args[0])?;
            Ok(Value::int(arr.len() as i64))
        });

        self.register("first", |args| {
            require_args("first", args, 1)?;
            let arr = require_array("first", &args[0])?;
            Ok(arr.first().cloned().unwrap_or(Value::Null))
        });

        self.register("last", |args| {
            require_args("last", args, 1)?;
            let arr = require_array("last", &args[0])?;
            Ok(arr.last().cloned().unwrap_or(Value::Null))
        });

        // ================================================================
        // Array functions (OPA-aligned additions)
        // ================================================================

        // sort(array) -> Array  — OPA: sort
        self.register("sort", |args| {
            require_args("sort", args, 1)?;
            let arr = require_array("sort", &args[0])?;
            let mut sorted = arr.to_vec();
            let mut sort_err: Option<OrdoError> = None;
            sorted.sort_by(|a, b| {
                a.compare(b).unwrap_or_else(|| {
                    sort_err = Some(OrdoError::eval_error("sort: cannot compare mixed types"));
                    std::cmp::Ordering::Equal
                })
            });
            if let Some(e) = sort_err {
                return Err(e);
            }
            Ok(Value::array(sorted))
        });

        // sort_desc(array) -> Array
        self.register("sort_desc", |args| {
            require_args("sort_desc", args, 1)?;
            let arr = require_array("sort_desc", &args[0])?;
            let mut sorted = arr.to_vec();
            let mut sort_err: Option<OrdoError> = None;
            sorted.sort_by(|a, b| {
                b.compare(a).unwrap_or_else(|| {
                    sort_err = Some(OrdoError::eval_error(
                        "sort_desc: cannot compare mixed types",
                    ));
                    std::cmp::Ordering::Equal
                })
            });
            if let Some(e) = sort_err {
                return Err(e);
            }
            Ok(Value::array(sorted))
        });

        // uniq(array) -> Array  (preserving insertion order)  — OPA: implicit via sets
        self.register("uniq", |args| {
            require_args("uniq", args, 1)?;
            let arr = require_array("uniq", &args[0])?;
            let mut seen: Vec<&Value> = Vec::new();
            let mut result: Vec<Value> = Vec::new();
            for v in arr {
                if !seen.contains(&v) {
                    seen.push(v);
                    result.push(v.clone());
                }
            }
            Ok(Value::array(result))
        });

        // flatten(array) -> Array  (one level deep)  — OPA: array.concat-like
        self.register("flatten", |args| {
            require_args("flatten", args, 1)?;
            let arr = require_array("flatten", &args[0])?;
            let mut result: Vec<Value> = Vec::new();
            for v in arr {
                match v {
                    Value::Array(inner) => result.extend(inner.iter().cloned()),
                    other => result.push(other.clone()),
                }
            }
            Ok(Value::array(result))
        });

        // concat(array, array) -> Array  — OPA: array.concat
        self.register("concat", |args| {
            require_args("concat", args, 2)?;
            let left = require_array("concat", &args[0])?;
            let right = require_array("concat", &args[1])?;
            let mut result = left.to_vec();
            result.extend(right.iter().cloned());
            Ok(Value::array(result))
        });

        // slice(array, start, end) -> Array  — OPA: array.slice
        self.register("slice", |args| {
            require_args("slice", args, 3)?;
            let arr = require_array("slice", &args[0])?;
            let start = (require_int("slice", &args[1])?.max(0) as usize).min(arr.len());
            let end = (require_int("slice", &args[2])?.max(0) as usize).min(arr.len());
            Ok(Value::array(arr[start..end].to_vec()))
        });

        // reverse(array) -> Array
        self.register("reverse", |args| {
            require_args("reverse", args, 1)?;
            let arr = require_array("reverse", &args[0])?;
            let mut result = arr.to_vec();
            result.reverse();
            Ok(Value::array(result))
        });

        // contains_any(array, needles) -> Bool
        self.register("contains_any", |args| {
            require_args("contains_any", args, 2)?;
            let arr = require_array("contains_any", &args[0])?;
            let needles = require_array("contains_any", &args[1])?;
            Ok(Value::bool(needles.iter().any(|needle| arr.contains(needle))))
        });

        // ================================================================
        // Set operations (OPA-aligned — treat Arrays as sets)
        // ================================================================

        // set_union(array, array) -> Array  — OPA: | on sets
        self.register("set_union", |args| {
            require_args("set_union", args, 2)?;
            let left = require_array("set_union", &args[0])?;
            let right = require_array("set_union", &args[1])?;
            let mut result = left.to_vec();
            for v in right {
                if !result.contains(v) {
                    result.push(v.clone());
                }
            }
            Ok(Value::array(result))
        });

        // set_intersection(array, array) -> Array  — OPA: & on sets
        self.register("set_intersection", |args| {
            require_args("set_intersection", args, 2)?;
            let left = require_array("set_intersection", &args[0])?;
            let right = require_array("set_intersection", &args[1])?;
            let result: Vec<Value> = left
                .iter()
                .filter(|v| right.contains(v))
                .cloned()
                .collect();
            Ok(Value::array(result))
        });

        // set_difference(array, array) -> Array  — OPA: - on sets
        self.register("set_difference", |args| {
            require_args("set_difference", args, 2)?;
            let left = require_array("set_difference", &args[0])?;
            let right = require_array("set_difference", &args[1])?;
            let result: Vec<Value> = left
                .iter()
                .filter(|v| !right.contains(v))
                .cloned()
                .collect();
            Ok(Value::array(result))
        });

        // is_subset(subset, superset) -> Bool
        self.register("is_subset", |args| {
            require_args("is_subset", args, 2)?;
            let subset = require_array("is_subset", &args[0])?;
            let superset = require_array("is_subset", &args[1])?;
            Ok(Value::bool(subset.iter().all(|v| superset.contains(v))))
        });

        // ================================================================
        // Object functions (OPA-aligned)
        // ================================================================

        // keys(object) -> Array  — OPA: object.keys
        self.register("keys", |args| {
            require_args("keys", args, 1)?;
            match &args[0] {
                Value::Object(m) => {
                    let ks: Vec<Value> = m.keys().map(|k| Value::string(k.as_ref())).collect();
                    Ok(Value::array(ks))
                }
                v => Err(OrdoError::type_error("object", v.type_name())),
            }
        });

        // values(object) -> Array
        self.register("values", |args| {
            require_args("values", args, 1)?;
            match &args[0] {
                Value::Object(m) => Ok(Value::array(m.values().cloned().collect())),
                v => Err(OrdoError::type_error("object", v.type_name())),
            }
        });

        // merge(obj1, obj2) -> Object  (shallow; obj2 wins on conflict)  — OPA: object.union
        self.register("merge", |args| {
            require_args("merge", args, 2)?;
            let left = require_object("merge", &args[0])?;
            let right = require_object("merge", &args[1])?;
            let mut result = left.clone();
            for (k, v) in right {
                result.insert(k.clone(), v.clone());
            }
            Ok(Value::Object(result))
        });

        // has_key(object, key) -> Bool  — OPA: object.get existence check
        self.register("has_key", |args| {
            require_args("has_key", args, 2)?;
            let obj = require_object("has_key", &args[0])?;
            let key = require_string("has_key", &args[1])?;
            Ok(Value::bool(obj.contains_key(key)))
        });

        // get_or(object, key, default) -> Value  — OPA: object.get(obj, key, default)
        self.register("get_or", |args| {
            require_args("get_or", args, 3)?;
            let obj = require_object("get_or", &args[0])?;
            let key = require_string("get_or", &args[1])?;
            Ok(obj.get(key).cloned().unwrap_or_else(|| args[2].clone()))
        });

        // ================================================================
        // Encoding / Decoding (OPA-aligned)
        // ================================================================

        // base64_encode(str) -> String  — OPA: base64.encode
        self.register("base64_encode", |args| {
            require_args("base64_encode", args, 1)?;
            let s = require_string("base64_encode", &args[0])?;
            Ok(Value::string(BASE64_STANDARD.encode(s.as_bytes())))
        });

        // base64_decode(str) -> String  — OPA: base64.decode
        self.register("base64_decode", |args| {
            require_args("base64_decode", args, 1)?;
            let s = require_string("base64_decode", &args[0])?;
            let bytes = BASE64_STANDARD
                .decode(s.as_bytes())
                .map_err(|e| OrdoError::eval_error(format!("base64_decode failed: {e}")))?;
            String::from_utf8(bytes)
                .map(Value::string)
                .map_err(|_| OrdoError::eval_error("base64_decode: result is not valid UTF-8"))
        });

        // json_parse(str) -> Value  — OPA: json.unmarshal
        self.register("json_parse", |args| {
            require_args("json_parse", args, 1)?;
            let s = require_string("json_parse", &args[0])?;
            let v: serde_json::Value = serde_json::from_str(s)
                .map_err(|e| OrdoError::eval_error(format!("json_parse failed: {e}")))?;
            serde_json::from_value(v).map_err(|e| {
                OrdoError::eval_error(format!("json_parse: value conversion failed: {e}"))
            })
        });

        // json_stringify(value) -> String  — OPA: json.marshal
        self.register("json_stringify", |args| {
            require_args("json_stringify", args, 1)?;
            serde_json::to_string(&args[0])
                .map(Value::string)
                .map_err(|e| OrdoError::eval_error(format!("json_stringify failed: {e}")))
        });

        // ================================================================
        // Type functions (original)
        // ================================================================

        self.register("type", |args| {
            require_args("type", args, 1)?;
            Ok(Value::string(args[0].type_name()))
        });

        self.register("is_null", |args| {
            require_args("is_null", args, 1)?;
            Ok(Value::bool(args[0].is_null()))
        });

        self.register("is_number", |args| {
            require_args("is_number", args, 1)?;
            Ok(Value::bool(args[0].is_number()))
        });

        self.register("is_string", |args| {
            require_args("is_string", args, 1)?;
            Ok(Value::bool(args[0].is_string()))
        });

        self.register("is_array", |args| {
            require_args("is_array", args, 1)?;
            Ok(Value::bool(args[0].is_array()))
        });

        // ================================================================
        // Type functions (OPA-aligned additions)
        // ================================================================

        // is_bool(v) -> Bool  — OPA: is_boolean
        self.register("is_bool", |args| {
            require_args("is_bool", args, 1)?;
            Ok(Value::bool(matches!(args[0], Value::Bool(_))))
        });

        // is_object(v) -> Bool  — OPA: is_object
        self.register("is_object", |args| {
            require_args("is_object", args, 1)?;
            Ok(Value::bool(matches!(args[0], Value::Object(_))))
        });

        // ================================================================
        // Conversion functions (original)
        // ================================================================

        self.register("to_int", |args| {
            require_args("to_int", args, 1)?;
            match &args[0] {
                Value::Int(n) => Ok(Value::int(*n)),
                Value::Float(n) => Ok(Value::int(*n as i64)),
                Value::String(s) => s
                    .parse::<i64>()
                    .map(Value::int)
                    .map_err(|_| OrdoError::eval_error(format!("Cannot convert '{}' to int", s))),
                Value::Bool(b) => Ok(Value::int(if *b { 1 } else { 0 })),
                v => Err(OrdoError::type_error(
                    "int, float, string, or bool",
                    v.type_name(),
                )),
            }
        });

        self.register("to_float", |args| {
            require_args("to_float", args, 1)?;
            match &args[0] {
                Value::Int(n) => Ok(Value::float(*n as f64)),
                Value::Float(n) => Ok(Value::float(*n)),
                Value::String(s) => s
                    .parse::<f64>()
                    .map(Value::float)
                    .map_err(|_| OrdoError::eval_error(format!("Cannot convert '{}' to float", s))),
                v => Err(OrdoError::type_error(
                    "int, float, or string",
                    v.type_name(),
                )),
            }
        });

        self.register("to_string", |args| {
            require_args("to_string", args, 1)?;
            Ok(Value::string(args[0].to_string()))
        });

        // ================================================================
        // Date/time functions (original)
        // ================================================================

        self.register("now", |_args| {
            Ok(Value::int(chrono::Utc::now().timestamp()))
        });

        self.register("now_millis", |_args| {
            Ok(Value::int(chrono::Utc::now().timestamp_millis()))
        });
    }

    /// Register a custom function
    pub fn register<F>(&mut self, name: impl Into<String>, f: F)
    where
        F: Fn(&[Value]) -> Result<Value> + Send + Sync + 'static,
    {
        self.functions.insert(name.into(), Arc::new(f));
    }

    /// Get a function by name
    pub fn get(&self, name: &str) -> Option<&FunctionFn> {
        self.functions.get(name)
    }

    /// Call a function by name
    ///
    /// Uses fast path for common built-in functions to avoid HashMap lookup overhead.
    #[inline]
    pub fn call(&self, name: &str, args: &[Value]) -> Result<Value> {
        // Fast path for most common functions - avoids HashMap lookup entirely
        match name {
            "len" => return Self::builtin_len_static(args),
            "sum" => return Self::builtin_sum_static(args),
            "max" => return Self::builtin_max_static(args),
            "min" => return Self::builtin_min_static(args),
            "abs" => return Self::builtin_abs_static(args),
            "count" => return Self::builtin_count_static(args),
            "is_null" => return Self::builtin_is_null_static(args),
            _ => {}
        }

        // Check custom functions first (local registry)
        if let Some(func) = self.functions.get(name) {
            return func(args);
        }

        // Fall back to global registry for other built-in functions
        if self.custom_only {
            if let Some(func) = global_builtin_registry().functions.get(name) {
                return func(args);
            }
        }

        Err(OrdoError::function_not_found(name.to_string()))
    }

    // ==================== Fast path implementations (static methods) ====================
    // These are static methods to avoid any self reference overhead in the hot path

    #[inline]
    fn builtin_len_static(args: &[Value]) -> Result<Value> {
        if args.len() != 1 {
            return Err(OrdoError::FunctionArgError {
                name: "len".into(),
                message: Cow::Owned(format!("expected 1 argument(s), got {}", args.len())),
            });
        }
        match &args[0] {
            Value::String(s) => Ok(Value::int(s.len() as i64)),
            Value::Array(a) => Ok(Value::int(a.len() as i64)),
            Value::Object(o) => Ok(Value::int(o.len() as i64)),
            v => Err(OrdoError::type_error(
                "string, array, or object",
                v.type_name(),
            )),
        }
    }

    #[inline]
    fn builtin_sum_static(args: &[Value]) -> Result<Value> {
        if args.len() != 1 {
            return Err(OrdoError::FunctionArgError {
                name: "sum".into(),
                message: Cow::Owned(format!("expected 1 argument(s), got {}", args.len())),
            });
        }
        let arr = args[0]
            .as_array()
            .ok_or_else(|| OrdoError::type_error("array", args[0].type_name()))?;

        let mut int_sum: i64 = 0;
        let mut float_sum: f64 = 0.0;
        let mut has_float = false;

        for v in arr {
            match v {
                Value::Int(n) => int_sum += n,
                Value::Float(n) => {
                    has_float = true;
                    float_sum += n;
                }
                _ => return Err(OrdoError::type_error("number", v.type_name())),
            }
        }

        if has_float {
            Ok(Value::float(int_sum as f64 + float_sum))
        } else {
            Ok(Value::int(int_sum))
        }
    }

    #[inline]
    fn builtin_max_static(args: &[Value]) -> Result<Value> {
        if args.is_empty() {
            return Err(OrdoError::FunctionArgError {
                name: "max".into(),
                message: "expected at least 1 argument".into(),
            });
        }
        let mut result = &args[0];
        for arg in &args[1..] {
            if arg.compare(result) == Some(std::cmp::Ordering::Greater) {
                result = arg;
            }
        }
        Ok(result.clone())
    }

    #[inline]
    fn builtin_min_static(args: &[Value]) -> Result<Value> {
        if args.is_empty() {
            return Err(OrdoError::FunctionArgError {
                name: "min".into(),
                message: "expected at least 1 argument".into(),
            });
        }
        let mut result = &args[0];
        for arg in &args[1..] {
            if arg.compare(result) == Some(std::cmp::Ordering::Less) {
                result = arg;
            }
        }
        Ok(result.clone())
    }

    #[inline]
    fn builtin_abs_static(args: &[Value]) -> Result<Value> {
        if args.len() != 1 {
            return Err(OrdoError::FunctionArgError {
                name: "abs".into(),
                message: Cow::Owned(format!("expected 1 argument(s), got {}", args.len())),
            });
        }
        match &args[0] {
            Value::Int(n) => n
                .checked_abs()
                .map(Value::int)
                .ok_or_else(|| OrdoError::eval_error("Integer overflow in abs()")),
            Value::Float(n) => Ok(Value::float(n.abs())),
            v => Err(OrdoError::type_error("number", v.type_name())),
        }
    }

    #[inline]
    fn builtin_count_static(args: &[Value]) -> Result<Value> {
        if args.len() != 1 {
            return Err(OrdoError::FunctionArgError {
                name: "count".into(),
                message: Cow::Owned(format!("expected 1 argument(s), got {}", args.len())),
            });
        }
        let arr = args[0]
            .as_array()
            .ok_or_else(|| OrdoError::type_error("array", args[0].type_name()))?;
        Ok(Value::int(arr.len() as i64))
    }

    #[inline]
    fn builtin_is_null_static(args: &[Value]) -> Result<Value> {
        if args.len() != 1 {
            return Err(OrdoError::FunctionArgError {
                name: "is_null".into(),
                message: Cow::Owned(format!("expected 1 argument(s), got {}", args.len())),
            });
        }
        Ok(Value::bool(args[0].is_null()))
    }

}

// ==================== Helper functions ====================

fn require_args(name: &str, args: &[Value], count: usize) -> Result<()> {
    if args.len() != count {
        Err(OrdoError::FunctionArgError {
            name: Cow::Owned(name.to_string()),
            message: Cow::Owned(format!(
                "expected {} argument(s), got {}",
                count,
                args.len()
            )),
        })
    } else {
        Ok(())
    }
}

fn require_string<'a>(_name: &str, value: &'a Value) -> Result<&'a str> {
    value
        .as_str()
        .ok_or_else(|| OrdoError::type_error("string", value.type_name()))
}

fn require_int(_name: &str, value: &Value) -> Result<i64> {
    value
        .as_int()
        .ok_or_else(|| OrdoError::type_error("int", value.type_name()))
}

fn require_float(_name: &str, value: &Value) -> Result<f64> {
    value
        .as_float()
        .ok_or_else(|| OrdoError::type_error("number", value.type_name()))
}

fn require_array<'a>(_name: &str, value: &'a Value) -> Result<&'a [Value]> {
    value
        .as_array()
        .map(|v| v.as_slice())
        .ok_or_else(|| OrdoError::type_error("array", value.type_name()))
}

fn require_object<'a>(
    _name: &str,
    value: &'a Value,
) -> Result<&'a hashbrown::HashMap<std::sync::Arc<str>, Value>> {
    match value {
        Value::Object(m) => Ok(m),
        v => Err(OrdoError::type_error("object", v.type_name())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> FunctionRegistry {
        FunctionRegistry::new()
    }

    // ================================================================
    // Original tests
    // ================================================================

    #[test]
    fn test_len() {
        let r = reg();
        assert_eq!(
            r.call("len", &[Value::string("hello")]).unwrap(),
            Value::int(5)
        );
        assert_eq!(
            r.call("len", &[Value::array(vec![Value::int(1), Value::int(2)])])
                .unwrap(),
            Value::int(2)
        );
    }

    #[test]
    fn test_string_functions() {
        let r = reg();
        assert_eq!(
            r.call("upper", &[Value::string("hello")]).unwrap(),
            Value::string("HELLO")
        );
        assert_eq!(
            r.call("lower", &[Value::string("HELLO")]).unwrap(),
            Value::string("hello")
        );
        assert_eq!(
            r.call("trim", &[Value::string("  hello  ")]).unwrap(),
            Value::string("hello")
        );
    }

    #[test]
    fn test_math_functions() {
        let r = reg();
        assert_eq!(r.call("abs", &[Value::int(-5)]).unwrap(), Value::int(5));
        assert_eq!(
            r.call("min", &[Value::int(3), Value::int(1), Value::int(2)])
                .unwrap(),
            Value::int(1)
        );
        assert_eq!(
            r.call("max", &[Value::int(3), Value::int(1), Value::int(2)])
                .unwrap(),
            Value::int(3)
        );
    }

    #[test]
    fn test_array_functions() {
        let r = reg();
        let arr = Value::array(vec![Value::int(1), Value::int(2), Value::int(3)]);
        assert_eq!(
            r.call("sum", std::slice::from_ref(&arr)).unwrap(),
            Value::int(6)
        );
        assert_eq!(
            r.call("avg", std::slice::from_ref(&arr)).unwrap(),
            Value::float(2.0)
        );
        assert_eq!(
            r.call("count", std::slice::from_ref(&arr)).unwrap(),
            Value::int(3)
        );
        assert_eq!(
            r.call("first", std::slice::from_ref(&arr)).unwrap(),
            Value::int(1)
        );
        assert_eq!(
            r.call("last", std::slice::from_ref(&arr)).unwrap(),
            Value::int(3)
        );
    }

    // ================================================================
    // String extension tests
    // ================================================================

    #[test]
    fn test_string_extended() {
        let r = reg();

        // split / join round-trip
        let parts = r
            .call("split", &[Value::string("a,b,c"), Value::string(",")])
            .unwrap();
        assert_eq!(
            parts,
            Value::array(vec![
                Value::string("a"),
                Value::string("b"),
                Value::string("c")
            ])
        );
        let joined = r.call("join", &[parts, Value::string("-")]).unwrap();
        assert_eq!(joined, Value::string("a-b-c"));

        // replace
        assert_eq!(
            r.call(
                "replace",
                &[
                    Value::string("hello world"),
                    Value::string("o"),
                    Value::string("0")
                ]
            )
            .unwrap(),
            Value::string("hell0 w0rld")
        );

        // trim_start / trim_end
        assert_eq!(
            r.call("trim_start", &[Value::string("  hi")]).unwrap(),
            Value::string("hi")
        );
        assert_eq!(
            r.call("trim_end", &[Value::string("hi  ")]).unwrap(),
            Value::string("hi")
        );

        // index_of
        assert_eq!(
            r.call("index_of", &[Value::string("hello"), Value::string("ll")])
                .unwrap(),
            Value::int(2)
        );
        assert_eq!(
            r.call("index_of", &[Value::string("hello"), Value::string("zz")])
                .unwrap(),
            Value::int(-1)
        );

        // repeat
        assert_eq!(
            r.call("repeat", &[Value::string("ab"), Value::int(3)])
                .unwrap(),
            Value::string("ababab")
        );

        // pad_left
        assert_eq!(
            r.call(
                "pad_left",
                &[Value::string("42"), Value::int(5), Value::string("0")]
            )
            .unwrap(),
            Value::string("00042")
        );
    }

    // ================================================================
    // Regex function tests
    // ================================================================

    #[test]
    fn test_regex_functions() {
        let r = reg();

        // regex_match
        assert_eq!(
            r.call(
                "regex_match",
                &[Value::string(r"^\d+$"), Value::string("12345")]
            )
            .unwrap(),
            Value::bool(true)
        );
        assert_eq!(
            r.call(
                "regex_match",
                &[Value::string(r"^\d+$"), Value::string("abc")]
            )
            .unwrap(),
            Value::bool(false)
        );

        // regex_find
        assert_eq!(
            r.call(
                "regex_find",
                &[Value::string(r"\d+"), Value::string("abc123def")]
            )
            .unwrap(),
            Value::string("123")
        );
        assert!(matches!(
            r.call(
                "regex_find",
                &[Value::string(r"\d+"), Value::string("abcdef")]
            )
            .unwrap(),
            Value::Null
        ));

        // regex_replace
        assert_eq!(
            r.call(
                "regex_replace",
                &[
                    Value::string("hello world"),
                    Value::string(r"\s+"),
                    Value::string("_")
                ]
            )
            .unwrap(),
            Value::string("hello_world")
        );

        // regex_split
        let parts = r
            .call(
                "regex_split",
                &[Value::string("one  two   three"), Value::string(r"\s+")],
            )
            .unwrap();
        assert_eq!(
            parts,
            Value::array(vec![
                Value::string("one"),
                Value::string("two"),
                Value::string("three")
            ])
        );
    }

    // ================================================================
    // Array extension tests
    // ================================================================

    #[test]
    fn test_array_extended() {
        let r = reg();
        let arr = Value::array(vec![Value::int(3), Value::int(1), Value::int(2)]);

        // sort / sort_desc
        assert_eq!(
            r.call("sort", &[arr.clone()]).unwrap(),
            Value::array(vec![Value::int(1), Value::int(2), Value::int(3)])
        );
        assert_eq!(
            r.call("sort_desc", &[arr.clone()]).unwrap(),
            Value::array(vec![Value::int(3), Value::int(2), Value::int(1)])
        );

        // uniq
        let dup = Value::array(vec![
            Value::int(1),
            Value::int(2),
            Value::int(1),
            Value::int(3),
        ]);
        assert_eq!(
            r.call("uniq", &[dup]).unwrap(),
            Value::array(vec![Value::int(1), Value::int(2), Value::int(3)])
        );

        // flatten
        let nested = Value::array(vec![
            Value::array(vec![Value::int(1), Value::int(2)]),
            Value::int(3),
            Value::array(vec![Value::int(4)]),
        ]);
        assert_eq!(
            r.call("flatten", &[nested]).unwrap(),
            Value::array(vec![
                Value::int(1),
                Value::int(2),
                Value::int(3),
                Value::int(4)
            ])
        );

        // concat
        let a = Value::array(vec![Value::int(1), Value::int(2)]);
        let b = Value::array(vec![Value::int(3), Value::int(4)]);
        assert_eq!(
            r.call("concat", &[a, b]).unwrap(),
            Value::array(vec![
                Value::int(1),
                Value::int(2),
                Value::int(3),
                Value::int(4)
            ])
        );

        // slice
        let s = Value::array((0..5).map(Value::int).collect());
        assert_eq!(
            r.call("slice", &[s, Value::int(1), Value::int(4)]).unwrap(),
            Value::array(vec![Value::int(1), Value::int(2), Value::int(3)])
        );

        // reverse
        let rev = Value::array(vec![Value::int(1), Value::int(2), Value::int(3)]);
        assert_eq!(
            r.call("reverse", &[rev]).unwrap(),
            Value::array(vec![Value::int(3), Value::int(2), Value::int(1)])
        );

        // contains_any
        let hay = Value::array(vec![Value::int(1), Value::int(2), Value::int(3)]);
        assert_eq!(
            r.call(
                "contains_any",
                &[
                    hay.clone(),
                    Value::array(vec![Value::int(5), Value::int(2)])
                ]
            )
            .unwrap(),
            Value::bool(true)
        );
        assert_eq!(
            r.call(
                "contains_any",
                &[hay, Value::array(vec![Value::int(5), Value::int(6)])]
            )
            .unwrap(),
            Value::bool(false)
        );
    }

    // ================================================================
    // Set operation tests
    // ================================================================

    #[test]
    fn test_set_operations() {
        let r = reg();
        let a = Value::array(vec![Value::int(1), Value::int(2), Value::int(3)]);
        let b = Value::array(vec![Value::int(2), Value::int(3), Value::int(4)]);

        // set_union
        let u = r.call("set_union", &[a.clone(), b.clone()]).unwrap();
        let mut u_vals: Vec<i64> = u
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_int().unwrap())
            .collect();
        u_vals.sort();
        assert_eq!(u_vals, vec![1, 2, 3, 4]);

        // set_intersection
        let i = r
            .call("set_intersection", &[a.clone(), b.clone()])
            .unwrap();
        let mut i_vals: Vec<i64> = i
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_int().unwrap())
            .collect();
        i_vals.sort();
        assert_eq!(i_vals, vec![2, 3]);

        // set_difference
        let d = r
            .call("set_difference", &[a.clone(), b.clone()])
            .unwrap();
        assert_eq!(d, Value::array(vec![Value::int(1)]));

        // is_subset
        let sub = Value::array(vec![Value::int(2), Value::int(3)]);
        assert_eq!(
            r.call("is_subset", &[sub.clone(), a.clone()]).unwrap(),
            Value::bool(true)
        );
        assert_eq!(
            r.call("is_subset", &[a.clone(), sub]).unwrap(),
            Value::bool(false)
        );
    }

    // ================================================================
    // Object function tests
    // ================================================================

    #[test]
    fn test_object_functions() {
        let r = reg();
        let obj: Value = serde_json::from_value(serde_json::json!({"a": 1, "b": 2})).unwrap();

        // keys
        let mut ks: Vec<String> = r
            .call("keys", &[obj.clone()])
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        ks.sort();
        assert_eq!(ks, vec!["a", "b"]);

        // values
        let mut vs: Vec<i64> = r
            .call("values", &[obj.clone()])
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_int().unwrap())
            .collect();
        vs.sort();
        assert_eq!(vs, vec![1, 2]);

        // has_key
        assert_eq!(
            r.call("has_key", &[obj.clone(), Value::string("a")])
                .unwrap(),
            Value::bool(true)
        );
        assert_eq!(
            r.call("has_key", &[obj.clone(), Value::string("z")])
                .unwrap(),
            Value::bool(false)
        );

        // get_or
        assert_eq!(
            r.call("get_or", &[obj.clone(), Value::string("a"), Value::int(99)])
                .unwrap(),
            Value::int(1)
        );
        assert_eq!(
            r.call("get_or", &[obj.clone(), Value::string("z"), Value::int(99)])
                .unwrap(),
            Value::int(99)
        );

        // merge
        let obj2: Value =
            serde_json::from_value(serde_json::json!({"b": 99, "c": 3})).unwrap();
        let merged = r.call("merge", &[obj.clone(), obj2]).unwrap();
        assert_eq!(
            r.call("get_or", &[merged.clone(), Value::string("a"), Value::Null])
                .unwrap(),
            Value::int(1)
        );
        assert_eq!(
            r.call("get_or", &[merged.clone(), Value::string("b"), Value::Null])
                .unwrap(),
            Value::int(99)
        );
        assert_eq!(
            r.call("get_or", &[merged, Value::string("c"), Value::Null])
                .unwrap(),
            Value::int(3)
        );
    }

    // ================================================================
    // Encoding function tests
    // ================================================================

    #[test]
    fn test_encoding_functions() {
        let r = reg();

        // base64 round-trip
        let encoded = r
            .call("base64_encode", &[Value::string("hello world")])
            .unwrap();
        assert_eq!(encoded, Value::string("aGVsbG8gd29ybGQ="));
        let decoded = r.call("base64_decode", &[encoded]).unwrap();
        assert_eq!(decoded, Value::string("hello world"));

        // json_stringify
        assert_eq!(
            r.call("json_stringify", &[Value::int(42)]).unwrap(),
            Value::string("42")
        );

        // json_parse / json_stringify round-trip
        let original = Value::array(vec![Value::int(1), Value::bool(true)]);
        let stringified = r.call("json_stringify", &[original.clone()]).unwrap();
        let parsed = r.call("json_parse", &[stringified]).unwrap();
        assert_eq!(parsed, original);

        // json_parse error
        assert!(r
            .call("json_parse", &[Value::string("not json")])
            .is_err());
    }

    // ================================================================
    // Math extension tests
    // ================================================================

    #[test]
    fn test_math_extended() {
        let r = reg();

        // pow
        assert_eq!(
            r.call("pow", &[Value::int(2), Value::int(10)]).unwrap(),
            Value::float(1024.0)
        );

        // sqrt
        assert_eq!(
            r.call("sqrt", &[Value::float(9.0)]).unwrap(),
            Value::float(3.0)
        );
        assert!(r.call("sqrt", &[Value::int(-1)]).is_err());

        // log
        let ln_e = r
            .call("log", &[Value::float(std::f64::consts::E)])
            .unwrap();
        assert!((ln_e.as_float().unwrap() - 1.0).abs() < 1e-10);
        assert!(r.call("log", &[Value::int(0)]).is_err());
    }

    // ================================================================
    // Type extension tests
    // ================================================================

    #[test]
    fn test_type_extended() {
        let r = reg();

        assert_eq!(
            r.call("is_bool", &[Value::bool(true)]).unwrap(),
            Value::bool(true)
        );
        assert_eq!(
            r.call("is_bool", &[Value::int(1)]).unwrap(),
            Value::bool(false)
        );

        let obj: Value = serde_json::from_value(serde_json::json!({"a": 1})).unwrap();
        assert_eq!(r.call("is_object", &[obj]).unwrap(), Value::bool(true));
        assert_eq!(
            r.call("is_object", &[Value::array(vec![])]).unwrap(),
            Value::bool(false)
        );
    }
}
