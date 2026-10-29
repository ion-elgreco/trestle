//! The pure-Python file of the generated package: `_enums.py`.
//!
//! Enums are better as Python source than as PyO3 classes. An `enum.StrEnum` (Python 3.11+)
//! member is a real `str`: it equals its name, works as a dict key for that name, and goes into
//! JSON as is. PyO3 enum classes cannot subclass `str`, and `pyo3-stub-gen` can only describe an
//! `enum.Enum` base. The model wrappers convert through these classes (see `model_conversions`).
//! Type checkers read the classes from the file directly, so it needs no stubs.

use crate::parsing::{CodeGenMetadata, EnumInfo};

use super::model_conversions::{hidden_value, python_class_name};

/// The `_enums.py` file: one `enum.StrEnum` class per enum in `enum_names` (the project enums,
/// see [`ModelClasses`](super::model_conversions::ModelClasses)).
pub(crate) fn enums_py(metadata: &CodeGenMetadata, enum_names: &[&String]) -> String {
    let mut out = String::from(
        "\"\"\"The enums of the API.\n\n\
         Each member is a `str` equal to its name, so a member and its name compare equal, and a\n\
         parameter takes either.\n\"\"\"\n\n\
         import enum\n",
    );
    for name in enum_names {
        let info = &metadata.enums[*name];
        out.push_str("\n\n");
        out.push_str(&enum_class(name, info));
    }
    out
}

fn enum_class(fq_name: &str, info: &EnumInfo) -> String {
    let class = python_class_name(fq_name);
    let mut out = format!("class {class}(enum.StrEnum):\n");
    if let Some(doc) = docstring(info.documentation.as_deref(), "    ") {
        out.push_str(&doc);
        out.push('\n');
    }
    for value in &info.values {
        if hidden_value(value) {
            continue;
        }
        // An explicit value: `enum.auto()` gives the lowercase name in a `StrEnum`.
        out.push_str(&format!("    {0} = \"{0}\"\n", value.name));
        if let Some(doc) = docstring(value.documentation.as_deref(), "    ") {
            out.push_str(&doc);
        }
    }
    out
}

/// A Python docstring at `indent`, or `None` when `doc` is empty.
fn docstring(doc: Option<&str>, indent: &str) -> Option<String> {
    let doc = doc?.trim();
    if doc.is_empty() {
        return None;
    }
    let lines: Vec<&str> = doc.lines().map(str::trim_end).collect();
    let text = lines
        .iter()
        .map(|l| l.trim_start())
        .collect::<Vec<_>>()
        .join(&format!("\n{indent}"))
        .replace("\"\"\"", "\\\"\\\"\\\"");
    if lines.len() == 1 {
        Some(format!("{indent}\"\"\"{text}\"\"\"\n"))
    } else {
        Some(format!("{indent}\"\"\"{text}\n{indent}\"\"\"\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docstring_keeps_lines_and_escapes_quotes() {
        assert_eq!(
            docstring(Some("One line."), "    ").as_deref(),
            Some("    \"\"\"One line.\"\"\"\n")
        );
        assert_eq!(
            docstring(Some(" First.\n Second \"\"\" here."), "  ").as_deref(),
            Some("  \"\"\"First.\n  Second \\\"\\\"\\\" here.\n  \"\"\"\n")
        );
        assert_eq!(docstring(Some("  \n "), "    "), None);
    }
}
