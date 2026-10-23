//! The pure-Python files of the generated package: `_enums.py` and `_errors.py`.
//!
//! Some Python types are better as Python source than as PyO3 classes:
//!
//! - **Enums.** An `enum.StrEnum` (Python 3.11+) member is a real `str`: it equals its name,
//!   works as a dict key for that name, and goes into JSON as is. PyO3 enum classes cannot
//!   subclass `str`, and `pyo3-stub-gen` can only describe an `enum.Enum` base. The model
//!   wrappers convert through these classes (see `model_conversions`).
//! - **Exceptions.** One base class with `message`, `code`, `status` and `details`, and one
//!   subclass per kind of HTTP error status. The generated error module raises them (see
//!   `errors`).
//!
//! Type checkers read the classes from these files directly, so they need no stubs.

use crate::codegen::PythonOptions;
use crate::parsing::{CodeGenMetadata, EnumInfo};

use super::model_conversions::{hidden_value, python_class_name};

/// The exception subclasses, one per kind of HTTP error status: `(name, statuses, doc)`.
/// `ServerError` takes every 5xx status.
pub(crate) const ERROR_CLASSES: &[(&str, &[u16], &str)] = &[
    (
        "BadRequestError",
        &[400],
        "The server answered `400 Bad Request`: the request is not valid.",
    ),
    (
        "UnauthenticatedError",
        &[401],
        "The server answered `401 Unauthorized`: the request has no valid credentials.",
    ),
    (
        "PermissionDeniedError",
        &[403],
        "The server answered `403 Forbidden`: the caller may not do this.",
    ),
    (
        "NotFoundError",
        &[404],
        "The server answered `404 Not Found`: the resource does not exist.",
    ),
    (
        "ConflictError",
        &[409],
        "The server answered `409 Conflict`: the resource exists or changed.",
    ),
    (
        "RateLimitError",
        &[429],
        "The server answered `429 Too Many Requests`.",
    ),
    (
        "ServerError",
        &[],
        "The server answered with a `5xx` status: it failed to do the request.",
    ),
];

/// The `_enums.py` file: one `enum.StrEnum` class per enum in `enum_names` (the project enums,
/// see [`ModelClasses`](super::model_conversions::ModelClasses)).
pub(crate) fn enums_py(
    metadata: &CodeGenMetadata,
    enum_names: &[&String],
    options: &PythonOptions,
) -> String {
    let mut out = String::from(
        "\"\"\"The enums of the API.\n\n\
         Each member is a `str` equal to its name, so a member and its name compare equal, and a\n\
         parameter takes either.\n\"\"\"\n\n\
         import enum\n",
    );
    for name in enum_names {
        let info = &metadata.enums[*name];
        out.push_str("\n\n");
        out.push_str(&enum_class(name, info, options));
    }
    out
}

fn enum_class(fq_name: &str, info: &EnumInfo, options: &PythonOptions) -> String {
    let class = python_class_name(fq_name);
    let mut out = format!("class {class}(enum.StrEnum):\n");
    if let Some(doc) = docstring(info.documentation.as_deref(), "    ") {
        out.push_str(&doc);
        out.push('\n');
    }
    for value in &info.values {
        if hidden_value(value, options) {
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

/// The `_errors.py` file: the base exception class and its subclasses.
pub(crate) fn errors_py(base: &str) -> String {
    let mut out = format!(
        r#""""The exceptions of the client."""

from typing import Any


class {base}(Exception):
    """An error answer of the server, or a request that got no answer.

    Attributes:
        message: The error message.
        code: The error code of the answer, such as ``"NOT_FOUND"``. ``None`` when the
            answer has none, or when the request got no answer.
        status: The HTTP status of the answer. ``None`` when the request got no answer.
        details: The details of the answer.
    """

    message: str
    code: str | None
    status: int | None
    details: list[Any]

    def __init__(
        self,
        message: str,
        code: str | None = None,
        status: int | None = None,
        details: list[Any] | None = None,
    ) -> None:
        super().__init__(message, code, status)
        self.message = message
        self.code = code
        self.status = status
        self.details = details or []

    def __str__(self) -> str:
        if self.code:
            return f"{{self.code}}: {{self.message}}"
        if self.status:
            return f"HTTP {{self.status}}: {{self.message}}"
        return self.message
"#
    );
    for (class, _, doc) in ERROR_CLASSES {
        out.push_str(&format!(
            "\n\nclass {class}({base}):\n    \"\"\"{doc}\"\"\"\n"
        ));
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
    fn errors_py_has_one_subclass_per_status_kind() {
        let src = errors_py("DemoError");
        assert!(src.starts_with("\"\"\"The exceptions of the client.\"\"\"\n"));
        assert!(src.contains("class DemoError(Exception):"));
        assert!(src.contains("class NotFoundError(DemoError):"));
        assert!(src.contains("class ServerError(DemoError):"));
        assert!(src.contains("return f\"{self.code}: {self.message}\""));
    }

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
