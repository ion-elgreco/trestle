//! Rewrites of the buffa model output.
//!
//! The buffa buf plugin writes the model types (`<package>.rs`) before trestle runs. Two of its
//! JSON choices are fixed in the plugin, so trestle rewrites its output in place:
//!
//! - **Field names.** buffa serializes the lowerCamelCase proto3-JSON names. With
//!   [`JsonFieldNames::Proto`] the default names become the snake_case proto names (camelCase is
//!   still read). A field with an explicit `json_name` that differs from the default keeps it.
//! - **64-bit integers.** buffa writes `int64`/`uint64` as quoted strings. With
//!   `int64_module = Some(path)` the models write numbers through the `json_int64` helper module
//!   (see [`RestClientOptions::json_int64_as_number`](crate::RestClientOptions)).
//!
//! The rewrites are idempotent: rewritten output does not match again. They leave alone the
//! files that trestle writes into the models directory itself.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::error::{Error, Result};
use crate::utils::strings::lower_camel;

/// The entries of the models directory that trestle writes itself.
const TRESTLE_FILES: &[&str] = &[
    "mod.rs",
    "labels.rs",
    "pyo3_impls.rs",
    crate::codegen::EXTRACTORS_SUBDIR,
];

/// How the JSON field names of one proto package are serialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsonFieldNames {
    /// lowerCamelCase field names, buffa's proto3-JSON default.
    #[default]
    Json,
    /// snake_case (proto) field names.
    Proto,
}

/// The rewrites to apply to the buffa model output.
#[derive(Debug, Clone, Default)]
pub struct ModelRewrites {
    /// Field-name style of every package not in [`packages`](Self::packages).
    pub field_names: JsonFieldNames,
    /// Field-name style per proto package (`my.pkg.v1`), overriding
    /// [`field_names`](Self::field_names). A package file is `<package>.rs`.
    pub packages: BTreeMap<String, JsonFieldNames>,
    /// Path of the `json_int64` helper module (e.g. `crate::models::json_int64`); when set,
    /// buffa's 64-bit integer helpers are pointed at it.
    pub int64_module: Option<String>,
}

impl ModelRewrites {
    /// Whether any rewrite is enabled.
    pub fn is_empty(&self) -> bool {
        self.field_names == JsonFieldNames::Json
            && self.packages.values().all(|n| *n == JsonFieldNames::Json)
            && self.int64_module.is_none()
    }

    /// The field-name style of the model file `file_name` (`<package>.rs`).
    fn field_names_for(&self, file_name: &str) -> JsonFieldNames {
        file_name
            .strip_suffix(".rs")
            .and_then(|package| self.packages.get(package))
            .copied()
            .unwrap_or(self.field_names)
    }

    /// Rewrite the source of the model file `file_name`.
    pub fn rewrite(&self, file_name: &str, src: &str) -> String {
        let mut out = src.to_string();
        if self.field_names_for(file_name) == JsonFieldNames::Proto {
            out = swap_serde_renames(&out);
            out = snakeify_serialize_entries(&out);
        }
        if let Some(module) = &self.int64_module {
            out = int64_helpers_to(&out, module);
        }
        out
    }

    /// Rewrite the model files under the models directory `dir` in place: every `.rs` file but
    /// the ones trestle writes. Returns the number of files changed.
    pub fn rewrite_dir(&self, dir: &Path) -> Result<usize> {
        if self.is_empty() || !dir.exists() {
            return Ok(0);
        }
        let mut changed = 0;
        for path in model_files(dir)? {
            let src = std::fs::read_to_string(&path).map_err(Error::Io)?;
            let file_name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let rewritten = self.rewrite(file_name, &src);
            if rewritten != src {
                std::fs::write(&path, rewritten).map_err(Error::Io)?;
                changed += 1;
            }
        }
        Ok(changed)
    }
}

/// Every `.rs` file under `dir`, recursively, in sorted order, without the [`TRESTLE_FILES`].
fn model_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).map_err(Error::Io)? {
            let path = entry.map_err(Error::Io)?.path();
            let trestle_file = d == dir
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| TRESTLE_FILES.contains(&n));
            if trestle_file {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Swap the `rename`/`alias` values of each `#[serde(...)]` attribute whose `rename` is the
/// default proto3-JSON name of its `alias` (the proto name), so serialization uses the proto
/// name. An explicit `json_name` (e.g. `metadata-location`) differs from the default and is kept.
fn swap_serde_renames(src: &str) -> String {
    // One `#[serde( ... )]` attribute; the body never contains `)]`.
    static SERDE_ATTR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"#\[serde\((?P<body>[^)]*)\)\]").expect("valid regex"));
    static RENAME: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"rename = "(?P<v>[^"]*)""#).expect("valid regex"));
    static ALIAS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"alias = "(?P<v>[^"]*)""#).expect("valid regex"));

    SERDE_ATTR
        .replace_all(src, |caps: &regex::Captures| {
            let whole = &caps[0];
            let body = &caps["body"];
            let (Some(rn), Some(al)) = (RENAME.captures(body), ALIAS.captures(body)) else {
                return whole.to_string();
            };
            let rename_val = rn["v"].to_string();
            let alias_val = al["v"].to_string();
            // Only the default JSON name is swapped. This also makes the rewrite idempotent: after
            // a swap `rename` is the proto name, which is not the default JSON name of the new
            // (camelCase) alias.
            if rename_val == alias_val || rename_val != lower_camel(&alias_val) {
                return whole.to_string();
            }
            let body = ALIAS.replace(body, format!(r#"alias = "{rename_val}""#));
            let body = RENAME.replace(&body, format!(r#"rename = "{alias_val}""#));
            format!("#[serde({body})]")
        })
        .into_owned()
}

/// Convert the `serialize_entry("camelCase", …)` JSON names in the `*View` `Serialize` impls to
/// snake_case. Only default proto3-JSON names are converted (see [`swap_serde_renames`]).
fn snakeify_serialize_entries(src: &str) -> String {
    static ENTRY: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"serialize_entry\("(?P<name>[^"]*)""#).expect("valid regex"));
    ENTRY
        .replace_all(src, |caps: &regex::Captures| {
            let name = &caps["name"];
            let snake = camel_to_snake(name);
            let is_default_json_name =
                name.starts_with(|c: char| c.is_ascii_lowercase()) && lower_camel(&snake) == name;
            if is_default_json_name {
                format!(r#"serialize_entry("{snake}""#)
            } else {
                caps[0].to_string()
            }
        })
        .into_owned()
}

/// Point buffa's 64-bit integer JSON helpers (`int64`, `opt_int64`, `uint64`, `opt_uint64`) at
/// `module`, which writes JSON numbers.
fn int64_helpers_to(src: &str, module: &str) -> String {
    static HELPER: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"::buffa::json_helpers::(?P<name>(?:opt_)?u?int64)\b").expect("valid regex")
    });
    HELPER
        .replace_all(src, |caps: &regex::Captures| {
            format!("{module}::{}", &caps["name"])
        })
        .into_owned()
}

/// Invert the proto3 JSON mapping: each uppercase letter becomes `_` + its lowercase.
fn camel_to_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for ch in name.chars() {
        if ch.is_ascii_uppercase() {
            out.push('_');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Put blank lines between the items of a pretty-printed Rust file. prettyplease prints none,
/// and rustfmt keeps blank lines but never adds them.
///
/// A blank line goes before an item (an attribute, a doc comment, or an item keyword) that
/// follows, at the same indentation, a closing `}` or the end of a `use` group; and after the
/// inner attributes and docs of the file. That covers the items of a module and the functions
/// of an `impl`. A file that already has these blank lines is returned unchanged.
pub fn space_items(src: &str) -> String {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = String::with_capacity(src.len() + src.len() / 16);
    for (i, line) in lines.iter().enumerate() {
        out.push_str(line);
        out.push('\n');
        if lines
            .get(i + 1)
            .is_some_and(|next| blank_between(line, next))
        {
            out.push('\n');
        }
    }
    out
}

/// Whether a blank line goes between the lines `line` and `next`.
fn blank_between(line: &str, next: &str) -> bool {
    let indent = |l: &str| l.len() - l.trim_start().len();
    if line.trim().is_empty() || next.trim().is_empty() || indent(line) != indent(next) {
        return false;
    }
    let (line, next) = (line.trim(), next.trim());
    let is_use = |l: &str| {
        ["use ", "pub use ", "pub(crate) use ", "pub(super) use "]
            .iter()
            .any(|p| l.starts_with(p))
    };
    let is_inner = |l: &str| l.starts_with("//!") || l.starts_with("#![");
    if is_inner(line) {
        return !is_inner(next);
    }
    if !starts_item(next) {
        return false;
    }
    match line {
        "}" => true,
        // A multi-line `use` ends with `};`: the next `use` stays in the group.
        "};" => !is_use(next),
        _ => is_use(line) && line.ends_with(';') && !is_use(next),
    }
}

/// Whether `line` (trimmed) starts an item, or the attributes or docs of one.
fn starts_item(line: &str) -> bool {
    const STARTS: &[&str] = &[
        "#[",
        "///",
        "/**",
        "pub ",
        "pub(",
        "fn ",
        "async fn ",
        "const fn ",
        "unsafe ",
        "extern ",
        "impl ",
        "impl<",
        "struct ",
        "enum ",
        "trait ",
        "mod ",
        "type ",
        "const ",
        "static ",
        "use ",
        "macro_rules!",
        "include!",
    ];
    STARTS.iter().any(|p| line.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proto() -> ModelRewrites {
        ModelRewrites {
            field_names: JsonFieldNames::Proto,
            ..Default::default()
        }
    }

    #[test]
    fn camel_to_snake_conversions() {
        assert_eq!(camel_to_snake("displayName"), "display_name");
        assert_eq!(camel_to_snake("nextPageToken"), "next_page_token");
        assert_eq!(camel_to_snake("name"), "name");
    }

    #[test]
    fn snakeifies_view_serialize_entry_literal() {
        let src = r#"                __map.serialize_entry("displayName", self.display_name)?;"#;
        let out = proto().rewrite("demo.v1.rs", src);
        assert!(out.contains(r#"serialize_entry("display_name", self.display_name)"#));
    }

    #[test]
    fn view_serialize_entry_explicit_names_untouched() {
        for src in [
            r#"__map.serialize_entry("name", self.name)?;"#,
            r#"__map.serialize_entry("metadata-location", self.metadata_location)?;"#,
            r#"__map.serialize_entry("Resources", self.resources)?;"#,
        ] {
            assert_eq!(proto().rewrite("demo.v1.rs", src), src);
        }
    }

    #[test]
    fn swaps_multiword_rename_alias_pair() {
        let src = r#"    #[serde(
        rename = "storageRoot",
        alias = "storage_root",
        skip_serializing_if = "::core::option::Option::is_none"
    )]
    pub storage_root: Option<String>,"#;
        let out = swap_serde_renames(src);
        assert!(out.contains(r#"rename = "storage_root""#));
        assert!(out.contains(r#"alias = "storageRoot""#));
        assert!(out.contains(r#"skip_serializing_if = "::core::option::Option::is_none""#));
    }

    #[test]
    fn single_word_rename_only_is_untouched() {
        let src = r#"    #[serde(rename = "name", skip_serializing_if = "Option::is_none")]"#;
        assert_eq!(swap_serde_renames(src), src);
    }

    #[test]
    fn explicit_json_name_is_kept() {
        // `json_name = "metadata-location"`: not the default JSON name of `metadata_location`.
        let src = r#"#[serde(rename = "metadata-location", alias = "metadata_location")]"#;
        assert_eq!(swap_serde_renames(src), src);
        let src = r#"#[serde(rename = "Resources", alias = "resources")]"#;
        assert_eq!(swap_serde_renames(src), src);
    }

    #[test]
    fn enum_and_scalar_with_helpers_preserved() {
        let src = r#"    #[serde(
        rename = "catalogType",
        alias = "catalog_type",
        with = "::buffa::json_helpers::opt_enum",
        skip_serializing_if = "::core::option::Option::is_none"
    )]"#;
        let out = swap_serde_renames(src);
        assert!(out.contains(r#"rename = "catalog_type""#));
        assert!(out.contains(r#"alias = "catalogType""#));
        assert!(out.contains(r#"with = "::buffa::json_helpers::opt_enum""#));
    }

    #[test]
    fn swap_is_idempotent_on_already_swapped_output() {
        let camel = r#"    #[serde(rename = "storageRoot", alias = "storage_root", skip_serializing_if = "x")]"#;
        let once = swap_serde_renames(camel);
        assert!(once.contains(r#"rename = "storage_root", alias = "storageRoot""#));
        assert_eq!(swap_serde_renames(&once), once);
    }

    #[test]
    fn non_field_serde_attr_is_untouched() {
        assert_eq!(swap_serde_renames("#[serde(default)]"), "#[serde(default)]");
    }

    #[test]
    fn package_override_keeps_camel_case() {
        let mut rewrites = proto();
        rewrites
            .packages
            .insert("demo.scim.v2".into(), JsonFieldNames::Json);
        let src = r#"#[serde(rename = "userName", alias = "user_name")]"#;
        assert_eq!(rewrites.rewrite("demo.scim.v2.rs", src), src);
        assert!(
            rewrites
                .rewrite("demo.v1.rs", src)
                .contains(r#"rename = "user_name""#)
        );
    }

    #[test]
    fn int64_helpers_point_at_module() {
        let rewrites = ModelRewrites {
            int64_module: Some("crate::models::json_int64".into()),
            ..Default::default()
        };
        let src = r#"with = "::buffa::json_helpers::opt_int64",
            ::buffa::json_helpers::int64::serialize(&self.0, __s)
            with = "::buffa::json_helpers::uint64",
            with = "::buffa::json_helpers::int32","#;
        let out = rewrites.rewrite("demo.v1.rs", src);
        assert!(out.contains(r#"with = "crate::models::json_int64::opt_int64""#));
        assert!(out.contains("crate::models::json_int64::int64::serialize(&self.0, __s)"));
        assert!(out.contains(r#"with = "crate::models::json_int64::uint64""#));
        assert!(out.contains(r#"with = "::buffa::json_helpers::int32""#));
        // Idempotent.
        assert_eq!(rewrites.rewrite("demo.v1.rs", &out), out);
    }

    #[test]
    fn rewrite_dir_leaves_trestle_files_alone() {
        let rewrites = ModelRewrites {
            int64_module: Some("crate::models::json_int64".into()),
            ..Default::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let src = r#"with = "::buffa::json_helpers::int64","#;
        std::fs::create_dir(dir.path().join("extractors")).unwrap();
        for file in ["demo.v1.rs", "mod.rs", "extractors/mod.rs"] {
            std::fs::write(dir.path().join(file), src).unwrap();
        }
        assert_eq!(rewrites.rewrite_dir(dir.path()).unwrap(), 1);
        let read = |file: &str| std::fs::read_to_string(dir.path().join(file)).unwrap();
        assert!(read("demo.v1.rs").contains("crate::models::json_int64::int64"));
        assert_eq!(read("mod.rs"), src);
        assert_eq!(read("extractors/mod.rs"), src);
    }

    #[test]
    fn space_items_separates_items_and_functions() {
        let src = "//! Docs.
use a::B;
use c::D;
/// A thing.
pub struct S {
    x: u8,
}
impl S {
    fn a(&self) {
        if true {
            f();
        }
        g();
    }
    #[inline]
    fn b(&self) -> u8 {
        match self.x {
            0 => {
                1
            }
            _ => 2,
        }
    }
}
";
        let spaced = space_items(src);
        assert_eq!(
            spaced,
            "//! Docs.

use a::B;
use c::D;

/// A thing.
pub struct S {
    x: u8,
}

impl S {
    fn a(&self) {
        if true {
            f();
        }
        g();
    }

    #[inline]
    fn b(&self) -> u8 {
        match self.x {
            0 => {
                1
            }
            _ => 2,
        }
    }
}
"
        );
        assert_eq!(space_items(&spaced), spaced);
    }
}
