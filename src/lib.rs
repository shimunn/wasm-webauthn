use std::{borrow::Cow, io::Read, ops::Deref};

use coset::{CborSerializable, CoseKey};
use derive_builder::Builder;
use js_sys::{Array, Uint8Array};
use serde::{Deserialize, Serialize};

mod error;

pub use error::*;
use tracing::{debug, instrument, trace};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;
pub use web_sys::UserVerificationRequirement;
use web_sys::{
    window, AuthenticatorAssertionResponse, AuthenticatorAttestationResponse,
    AuthenticatorSelectionCriteria, CredentialCreationOptions, CredentialRequestOptions,
    PublicKeyCredential, PublicKeyCredentialCreationOptions, PublicKeyCredentialDescriptor,
    PublicKeyCredentialRequestOptions, PublicKeyCredentialRpEntity, PublicKeyCredentialUserEntity,
    Window,
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename = "public-key")]
pub struct PubKeyCredParams {
    alg: i32,
}

impl PubKeyCredParams {
    const fn nistp256() -> Self {
        Self { alg: -7 }
    }
}
impl Default for PubKeyCredParams {
    fn default() -> Self {
        Self::nistp256()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UserVerification {
    Required,
    Preferred,
    Discouraged,
}

#[derive(Debug, Clone, Builder)]
pub struct MakeCredentialArgs<'a> {
    /// Challenge to be included within `clientDataJson`
    /// only relevant if attestion is requested
    #[builder(default)]
    pub challenge: Vec<u8>,
    #[builder(default)]
    pub algorithms: Cow<'a, [PubKeyCredParams]>,
    /// A string which indicates the relying party's identifier (ex. "login.example.org"). If this option is not provided, the client will use the current origin's domain.
    #[builder(default)]
    pub rp_id: Option<String>,
    #[builder(default = "UserVerificationRequirement::Discouraged")]
    pub uv: UserVerificationRequirement,
    #[builder(default)]
    pub resident_key: bool,
    #[builder(default)]
    pub timeout: Option<u32>,
    #[builder(default)]
    pub user_id: Option<Vec<u8>>,
    #[builder(default)]
    pub user_name: Option<String>,
    #[builder(default)]
    pub user_display_name: Option<String>,
}
#[instrument(skip(reader))]
fn read_fixed<const N: usize, R: Read>(mut reader: R) -> Result<[u8; N]> {
    let mut arr = [0u8; N];
    let res = reader.read_exact(&mut arr[..]);
    trace!(%N, ?res, "read");
    Ok(arr)
}

#[instrument(skip(reader))]
fn read_vec<R: Read>(mut reader: R) -> Result<Vec<u8>> {
    let len: [u8; 2] = read_fixed(&mut reader)?;
    let len = u16::from_be_bytes(len) as usize;
    let mut vec = vec![0u8; len];
    let res = reader.read_exact(&mut vec[..]);
    trace!(%len, ?res, "read");
    Ok(vec)
}

impl MakeCredentialArgs<'_> {
    pub async fn make_credential(&self) -> Result<MakeCredentialResponse> {
        let window = get_window()?;
        let navigator = window.navigator();
        let challenge = Uint8Array::from(self.challenge.as_slice());
        let default_alg = &[PubKeyCredParams::nistp256()][..];
        let algorithms = serde_wasm_bindgen::to_value(if self.algorithms.is_empty() {
            default_alg
        } else {
            &self.algorithms
        })?;
        let mut options = CredentialCreationOptions::new();
        let user = PublicKeyCredentialUserEntity::new(
            self.user_name.as_deref().unwrap_or_default(),
            self.user_display_name.as_deref().unwrap_or_default(),
            &Uint8Array::from(self.user_id.as_deref().unwrap_or(&[0u8])).into(),
        );
        let mut selection = AuthenticatorSelectionCriteria::new();
        selection.require_resident_key(self.resident_key);
        selection.user_verification(self.uv);
        let rp = PublicKeyCredentialRpEntity::new(&match self.rp_id.as_deref() {
            Some(rp) => Cow::Borrowed(rp),
            None => Cow::Owned(window.location().hostname()?),
        });
        options.public_key(
            PublicKeyCredentialCreationOptions::new(&challenge, &algorithms, &rp, &user)
                .authenticator_selection(&selection),
        );
        // Request credential
        let response =
            JsFuture::from(navigator.credentials().create_with_options(&options)?).await?;
        let public_key_response = PublicKeyCredential::from(response);

        let attestation_response =
            AuthenticatorAttestationResponse::from(JsValue::from(public_key_response.response()));
        let attestation_object: AttestationObject = ciborium::de::from_reader(
            &Uint8Array::new(&attestation_response.attestation_object()).to_vec()[..],
        )?;
        let (id, public_key) = {
            let mut reader = &attestation_object.auth_data[..];
            //TODO: return
            let _rp_id_hash: [u8; 32] = read_fixed(&mut reader)?;
            let _flags = read_fixed::<1, _>(&mut reader)?[0];
            let _counter = u32::from_be_bytes(read_fixed::<4, _>(&mut reader)?);
            let _aaguid = read_fixed::<16, _>(&mut reader)?;

            let id = CredentialID(read_vec(&mut reader)?);
            let public_key = CoseKey::from_slice(reader)?;
            (id, public_key)
        };
        let credential = Credential {
            id,
            public_key: Some(public_key),
        };
        Ok(MakeCredentialResponse { credential })
    }
}

fn get_window() -> Result<Window> {
    window().ok_or(Error::ContextUnavailable)
}
#[derive(Deserialize)]
#[serde(tag = "fmt", rename = "packed")]
struct AttestationObject {
    #[serde(rename = "authData", with = "serde_bytes")]
    auth_data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialID(pub Vec<u8>);

impl Deref for CredentialID {
    type Target = Vec<u8>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct Credential {
    pub id: CredentialID,
    pub public_key: Option<CoseKey>,
}

impl From<CredentialID> for Credential {
    fn from(id: CredentialID) -> Self {
        Self {
            id,
            public_key: None,
        }
    }
}

pub struct MakeCredentialResponse {
    pub credential: Credential,
}

#[derive(Debug, Builder)]
pub struct GetAssertionArgs {
    /// List of credentials, will attempt to use an resident key if `None` is specified
    #[builder(default)]
    pub credentials: Option<Vec<Credential>>,
    #[builder(default)]
    pub rp_id: Option<String>,
    #[builder(default = "UserVerificationRequirement::Discouraged")]
    pub uv: UserVerificationRequirement,
    #[builder(default)]
    pub timeout: Option<u32>,
    #[builder(default)]
    pub challenge: Vec<u8>,
}

impl GetAssertionArgs {
    pub async fn get_assertion(&self) -> Result<GetAssertionResponse> {
        let window = get_window()?;
        let mut request_options =
            PublicKeyCredentialRequestOptions::new(&Uint8Array::from(self.challenge.as_slice()));
        request_options.rp_id(&match self.rp_id.as_deref() {
            Some(rp) => Cow::Borrowed(rp),
            None => Cow::Owned(window.location().hostname()?),
        });
        if let Some(ref credentials) = self.credentials {
            request_options.allow_credentials(&JsValue::from(Array::from_iter(
                credentials.iter().map(|Credential { id, .. }| {
                    PublicKeyCredentialDescriptor::new(
                        &Uint8Array::from(id.as_slice()),
                        web_sys::PublicKeyCredentialType::PublicKey,
                    )
                }),
            )));
        }
        request_options.user_verification(self.uv);
        if let Some(timeout) = self.timeout {
            request_options.timeout(timeout);
        }

        let mut options = CredentialRequestOptions::new();
        options.public_key(&request_options);

        fn dbg<T: std::fmt::Debug>(v: T) -> T {
            debug!(value = ?v, "dbg");
            v
        }

        let response = dbg(JsFuture::from(
            window
                .navigator()
                .credentials()
                .get_with_options(&options)?,
        )
        .await)?;

        let public_key_response = PublicKeyCredential::from(response);

        let assertion_response =
            AuthenticatorAssertionResponse::from(JsValue::from(&public_key_response.response()));
        let authenticator_data = Uint8Array::new(&assertion_response.authenticator_data()).to_vec();
        let signature = Uint8Array::new(&assertion_response.signature()).to_vec();
        let client_data_json =
            String::from_utf8(Uint8Array::new(&assertion_response.client_data_json()).to_vec())?;
        let (flags, counter) = {
            let mut reader = authenticator_data.as_slice();
            let _rp_id_hash = read_fixed::<32, _>(&mut reader)?;
            let flags = read_fixed::<1, _>(&mut reader)?[0];
            let counter = u32::from_be_bytes(read_fixed::<4, _>(&mut reader)?);
            (flags, counter)
        };
        // TODO: read credential ID which has been used
        Ok(GetAssertionResponse {
            signature,
            client_data_json,
            flags,
            counter,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetAssertionResponse {
    pub signature: Vec<u8>,
    pub client_data_json: String,
    pub flags: u8,
    pub counter: u32,
}
