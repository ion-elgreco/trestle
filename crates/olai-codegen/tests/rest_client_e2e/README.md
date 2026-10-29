# REST client end-to-end fixture

Opt-in proof that the REST client and Python options of `olai-codegen` produce code that
compiles, passes `clippy -D warnings`, and behaves as configured against a real HTTP server.

The fixture generates, from `proto/demo/v1/svc.proto`, with every option on (see `config.rs`):

- the buffa models with the `trestle` rewrites (snake_case JSON names, int64 as numbers);
- the Rust client with the generated `reqwest` transport and error module
  (`rest-client-e2e`);
- the Python bindings with frozen models, `str` enums, `_async` twins, keyword-only
  arguments, the generated exceptions and `pyo3-stub-gen` annotations (`py/`,
  `rest-client-e2e-py`).

The tests run both against a local axum server; the Python tests run the generated API in an
embedded interpreter and check the stubs that `pyo3-stub-gen` writes.

## Running

Requires `protoc` on `PATH` and a Python with its shared library (e.g. Homebrew python3.12):

```sh
PYO3_PYTHON=/opt/homebrew/bin/python3.12 \
  cargo test --manifest-path crates/olai-codegen/tests/rest_client_e2e/Cargo.toml --workspace
```

The crate is excluded from the root workspace, like `buffa_e2e`.
