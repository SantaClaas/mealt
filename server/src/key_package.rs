use axum::{
    async_trait,
    body::Bytes,
    extract::{FromRequest, Request},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use openmls::prelude::*;
pub(crate) struct KeyPackage(pub(crate) KeyPackageIn);

#[async_trait]
impl<S> FromRequest<S> for KeyPackage
where
    Bytes: FromRequest<S>,
    S: Send + Sync,
{
    type Rejection = Response;

    async fn from_request(request: Request, state: &S) -> Result<Self, Self::Rejection> {
        //TODO stream bytes directly into KeyPackageIn::tls_deserialize without buffering everything first
        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(IntoResponse::into_response)?;

        let mut bytes = bytes.as_ref();
        let package = openmls::key_packages::KeyPackageIn::tls_deserialize(&mut bytes);
        match package {
            Ok(package) => Ok(KeyPackage(package)),
            //TODO log error if it doesn't contain PII
            Err(_) => Err(StatusCode::BAD_REQUEST.into_response()),
        }
    }
}
