// @generated — do not edit by hand.
#![allow(dead_code, unused_imports, clippy::too_many_arguments)]

#[allow(dead_code, unused_imports, clippy::too_many_arguments)]
pub mod catalog;
#[allow(dead_code, unused_imports, clippy::too_many_arguments)]
pub mod schema;
use std::collections::HashMap;
use futures::stream::TryStreamExt;
use pyo3::prelude::*;
use example_client::ExampleClient;
use crate::error::{PyExampleError, PyExampleResult};
use crate::runtime::get_runtime;
use example_common::models::catalog::v1::*;
use example_common::models::schemas::v1::*;
use example_common::models::tags::v1::*;
use example_common::models::*;
use crate::codegen::catalog::PyCatalogClient;
use crate::codegen::schema::PySchemaClient;

#[pyclass(name = "ExampleClient")]
pub struct PyExampleClient {
    pub(crate) client: ExampleClient,
}

#[pymethods]
impl PyExampleClient {
    #[new]
    #[pyo3(signature = (base_url, token = None))]
    pub fn new(base_url: String, token: Option<String>) -> PyResult<Self> {
        let base_url = base_url.parse().map_err(PyExampleError::from)?;
        let client = match token {
            Some(token) => ExampleClient::new_with_token(base_url, token),
            None => ExampleClient::new_unauthenticated(base_url),
        };
        Ok(Self { client })
    }

    #[pyo3(signature = (catalog = None))]
    pub fn create_catalog(
        &self,
        py: Python,
        catalog: ::core::option::Option<PyCatalog>,
    ) -> PyExampleResult<PyCatalog> {
        let request = self
            .client
            .create_catalog()
            .with_catalog(catalog.map(::core::convert::Into::into));
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyCatalog::from(result))
        })
    }

    #[pyo3(signature = (max_results))]
    pub fn list_catalogs(
        &self,
        py: Python,
        max_results: i32,
    ) -> PyExampleResult<::std::vec::Vec<PyCatalog>> {
        let request = self.client.list_catalogs(max_results, page_token);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime
                .block_on(async move {
                    request.into_stream().try_collect::<::std::vec::Vec<_>>().await
                })?;
            Ok::<
                _,
                PyExampleError,
            >(result.into_iter().map(PyCatalog::from).collect::<::std::vec::Vec<_>>())
        })
    }

    #[pyo3(signature = (catalog_id))]
    pub fn generate_catalog_token(
        &self,
        py: Python,
        catalog_id: String,
    ) -> PyExampleResult<PyCatalogToken> {
        let request = self.client.generate_catalog_token(catalog_id);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyCatalogToken::from(result))
        })
    }

    #[pyo3(signature = (tags, max_results))]
    pub fn list_by_tags(
        &self,
        py: Python,
        tags: Vec<String>,
        max_results: i32,
    ) -> PyExampleResult<PyListByTagsResponse> {
        let request = self.client.list_by_tags(tags, max_results);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyListByTagsResponse::from(result))
        })
    }

    #[pyo3(signature = (catalog_type))]
    pub fn list_by_catalog_type(
        &self,
        py: Python,
        catalog_type: PyCatalogType,
    ) -> PyExampleResult<PyListByTagsResponse> {
        let request = self.client.list_by_catalog_type(catalog_type.into());
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyListByTagsResponse::from(result))
        })
    }

    #[pyo3(signature = (name, catalog_name, schema_type))]
    pub fn create_schema(
        &self,
        py: Python,
        name: String,
        catalog_name: String,
        schema_type: PySchemaType,
    ) -> PyExampleResult<PySchema> {
        let request = self.client.create_schema(name, catalog_name, schema_type.into());
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PySchema::from(result))
        })
    }

    #[pyo3(signature = (catalog_name, max_results))]
    pub fn list_schemas(
        &self,
        py: Python,
        catalog_name: String,
        max_results: i32,
    ) -> PyExampleResult<::std::vec::Vec<PySchema>> {
        let request = self.client.list_schemas(catalog_name, max_results, page_token);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime
                .block_on(async move {
                    request.into_stream().try_collect::<::std::vec::Vec<_>>().await
                })?;
            Ok::<
                _,
                PyExampleError,
            >(result.into_iter().map(PySchema::from).collect::<::std::vec::Vec<_>>())
        })
    }

    #[pyo3(signature = (entity_type, entity_name, max_results, page_token))]
    pub fn list_tag_assignments(
        &self,
        py: Python,
        entity_type: String,
        entity_name: String,
        max_results: i32,
        page_token: String,
    ) -> PyExampleResult<PyListTagAssignmentsResponse> {
        let request = self
            .client
            .list_tag_assignments(entity_type, entity_name, max_results, page_token);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyListTagAssignmentsResponse::from(result))
        })
    }

    #[pyo3(signature = (entity_type, entity_name, tag = None))]
    pub fn create_tag_assignment(
        &self,
        py: Python,
        entity_type: String,
        entity_name: String,
        tag: ::core::option::Option<PyTagAssignment>,
    ) -> PyExampleResult<PyTagAssignment> {
        let request = self
            .client
            .create_tag_assignment(entity_type, entity_name)
            .with_tag(tag.map(::core::convert::Into::into));
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyTagAssignment::from(result))
        })
    }

    #[pyo3(signature = (entity_type, entity_name, tag_key))]
    pub fn fetch_tag_assignment(
        &self,
        py: Python,
        entity_type: String,
        entity_name: String,
        tag_key: String,
    ) -> PyExampleResult<PyTagAssignment> {
        let request = self.client.get_tag_assignment(entity_type, entity_name, tag_key);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyTagAssignment::from(result))
        })
    }

    #[pyo3(signature = (entity_type, entity_name, tag_key))]
    pub fn peek_tag_assignment(
        &self,
        py: Python,
        entity_type: String,
        entity_name: String,
        tag_key: String,
    ) -> PyExampleResult<PyTagAssignment> {
        let request = self.client.peek_tag_assignment(entity_type, entity_name, tag_key);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyTagAssignment::from(result))
        })
    }

    #[pyo3(signature = (entity_type, entity_name, tag_key))]
    pub fn delete_tag_assignment(
        &self,
        py: Python,
        entity_type: String,
        entity_name: String,
        tag_key: String,
    ) -> PyExampleResult<PyDeleteTagAssignmentResponse> {
        let request = self
            .client
            .delete_tag_assignment(entity_type, entity_name, tag_key);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(PyDeleteTagAssignmentResponse::from(result))
        })
    }

    #[pyo3(signature = (entity_type, entity_name, tag_key))]
    pub fn touch_tag_assignment(
        &self,
        py: Python,
        entity_type: String,
        entity_name: String,
        tag_key: String,
    ) -> PyExampleResult<()> {
        let request = self
            .client
            .touch_tag_assignment(entity_type, entity_name, tag_key);
        let runtime = get_runtime(py)?;
        py.detach(|| {
            #[allow(clippy::let_unit_value)]
            let result = runtime.block_on(request.into_future())?;
            Ok::<_, PyExampleError>(result)
        })
    }

    pub fn catalog(&self, catalog_name: String) -> PyCatalogClient {
        PyCatalogClient {
            client: self.client.catalog(catalog_name),
        }
    }

    pub fn schema(&self, catalog_name: String, schema_name: String) -> PySchemaClient {
        let full_name = format!("{}.{}", catalog_name, schema_name);
        PySchemaClient {
            client: self.client.schema_from_full_name(full_name),
        }
    }
}

/// Add the generated classes to the extension module `module`.
pub fn register(
    module: &::pyo3::Bound<'_, ::pyo3::types::PyModule>,
) -> ::pyo3::PyResult<()> {
    module.add_class::<PyExampleClient>()?;
    module.add_class::<PyCatalogType>()?;
    module.add_class::<PyAzureConfig>()?;
    module.add_class::<PyCatalog>()?;
    module.add_class::<PyCatalogEvent>()?;
    module.add_class::<PyCatalogStatus>()?;
    module.add_class::<PyCatalogToken>()?;
    module.add_class::<PyCreateCatalogRequest>()?;
    module.add_class::<PyDeleteCatalogRequest>()?;
    module.add_class::<PyDeleteCatalogResponse>()?;
    module.add_class::<PyGenerateCatalogTokenRequest>()?;
    module.add_class::<PyGetCatalogRequest>()?;
    module.add_class::<PyGetCatalogStatusRequest>()?;
    module.add_class::<PyListByCatalogTypeRequest>()?;
    module.add_class::<PyListByTagsRequest>()?;
    module.add_class::<PyListByTagsResponse>()?;
    module.add_class::<PyListCatalogsRequest>()?;
    module.add_class::<PyListCatalogsResponse>()?;
    module.add_class::<PyS3Config>()?;
    module.add_class::<PyStorageConfig>()?;
    module.add_class::<PyStreamCatalogEventsRequest>()?;
    module.add_class::<PyUpdateCatalogRequest>()?;
    module.add_class::<PyCatalogClient>()?;
    module.add_class::<PyGetSchemaRequestView>()?;
    module.add_class::<PySchemaType>()?;
    module.add_class::<PyCreateSchemaRequest>()?;
    module.add_class::<PyDeleteSchemaRequest>()?;
    module.add_class::<PyDeleteSchemaResponse>()?;
    module.add_class::<PyGetSchemaRequest>()?;
    module.add_class::<PyListSchemasRequest>()?;
    module.add_class::<PyListSchemasResponse>()?;
    module.add_class::<PySchema>()?;
    module.add_class::<PyUpdateSchemaRequest>()?;
    module.add_class::<PySchemaClient>()?;
    module.add_class::<PyCreateTagAssignmentRequest>()?;
    module.add_class::<PyDeleteTagAssignmentRequest>()?;
    module.add_class::<PyDeleteTagAssignmentResponse>()?;
    module.add_class::<PyGetTagAssignmentRequest>()?;
    module.add_class::<PyListTagAssignmentsRequest>()?;
    module.add_class::<PyListTagAssignmentsResponse>()?;
    module.add_class::<PyTagAssignment>()?;
    module.add_class::<PyTouchTagAssignmentRequest>()?;
    Ok(())
}
