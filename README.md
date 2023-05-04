### Webauthn for wasm

### Usage example

```rust
use wasm_webauthn::*;

async fn register() -> Credential {
    let MakeCredentialResponse { credential } = MakeCredentialArgsBuilder::default()
        .rp_id(Some("example.com".to_string()))
        .challenge([42u8; 32].to_vec())
        .uv(UserVerificationRequirement::Required)
        .build().expect("invalid args")
        .make_credential().await
        .expect("make credential");
    credential
}

async fn get_assertion(credential: Credential) {
    let GetAssertionResponse {
        signature,
        client_data_json,
        flags,
        counter,
    } = GetAssertionArgsBuilder::default()
        .credentials(Some(vec![credential.into()]))
        .challenge("Hello World".as_bytes().to_vec())
        .build()
        .expect("invalid args")
        .get_assertion().await
        .expect("get assertion");
}
```
