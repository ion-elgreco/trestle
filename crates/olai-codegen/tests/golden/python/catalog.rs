// @generated — do not edit by hand.
use std::collections::HashMap;
use pyo3::prelude::*;
use example_client::CatalogClient;
use example_common::models::catalog::v1::*;
use example_common::models::*;
use crate::error::{PyExampleError, PyExampleResult};
use crate::runtime::get_runtime;

#[pyclass(name = "CatalogClient")]
pub struct PyCatalogClient {
    pub(crate) client: CatalogClient,
}

#[pymethods]
impl PyCatalogClient {
    pub fn get(&self, py: Python) -> PyExampleResult<PyCatalog> {
        let request = self.client.get();
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyCatalog::from(result))
        })
    }

    #[pyo3(signature = (catalog = None))]
    pub fn update(
        &self,
        py: Python,
        catalog: ::core::option::Option<PyCatalog>,
    ) -> PyExampleResult<PyCatalog> {
        let request = self
            .client
            .update()
            .with_catalog(catalog.map(::core::convert::Into::into));
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyCatalog::from(result))
        })
    }

    pub fn delete(&self, py: Python) -> PyExampleResult<()> {
        let request = self.client.delete();
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(result)
        })
    }

    #[pyo3(signature = (if_none_match = None))]
    pub fn get_catalog_status(
        &self,
        py: Python,
        if_none_match: Option<String>,
    ) -> PyExampleResult<PyCatalogStatus> {
        let request = self.client.get_catalog_status().with_if_none_match(if_none_match);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyCatalogStatus::from(result))
        })
    }

    pub fn stream_catalog_events(
        &self,
        py: Python,
    ) -> PyExampleResult<Vec<PyCatalogEvent>> {
        let request = self.client.stream_catalog_events();
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<
                _,
                PyExampleError,
            >(result.into_iter().map(PyCatalogEvent::from).collect::<Vec<_>>())
        })
    }

    #[pyo3(signature = (range = None))]
    pub fn download_catalog_export(
        &self,
        py: Python,
        range: Option<String>,
    ) -> PyExampleResult<PyCatalogExportChunk> {
        let request = self.client.download_catalog_export().with_range(range);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime
                .block_on(async move {
                    let mut chunks = match request.into_future().await {
                        Ok(chunks) => chunks,
                        Err(e) => return Err(e),
                    };
                    let mut whole = None;
                    while let Some(chunk) = ::futures::StreamExt::next(&mut chunks).await
                    {
                        let chunk = chunk?;
                        match &mut whole {
                            None => whole = Some(chunk),
                            Some(whole) => whole.contents.extend(chunk.contents),
                        }
                    }
                    Ok(whole.unwrap_or_default())
                })?;
            Ok::<_, PyExampleError>(PyCatalogExportChunk::from(result))
        })
    }

    #[pyo3(signature = (contents))]
    pub fn upload_catalog_import(
        &self,
        py: Python,
        contents: ::pyo3::pybacked::PyBackedBytes,
    ) -> PyExampleResult<PyDeleteCatalogResponse> {
        let request = self.client.upload_catalog_import(contents.to_vec());
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyDeleteCatalogResponse::from(result))
        })
    }
}

impl PyCatalogClient {
    pub fn new(client: CatalogClient) -> Self {
        Self { client }
    }
}
