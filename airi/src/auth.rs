// WARNING: SHA1 LIB IS CRYPTOGRAPHICALLY INSECURE, LOOKING FOR REPLACEMENT

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha1::{Digest, Sha1};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AuthError {
    #[error("Authentication servers are down")]
    FailedResponse,
    #[error("Failed to verify username")]
    UnverifiedUsername,
    #[error("You are banned from Authentication servers: \"{0}\"")]
    Banned(String),
    // #[error("Texture Error {0}")]
    // TextureError(TextureError),
    #[error("You have disallowed actions from Authentication servers")]
    DisallowedAction,
    #[error("Your Xbox Profile has multiplayer disabled")]
    InsufficientPrivileges,
    #[error("Failed to parse JSON response")]
    FailedParse,
    #[error("Forbidden because of malformed request body")]
    Forbidden,
    #[error("Xsts auth failed: {0}")]
    XSTSFailure(String),
    // #[error("Unknown Status Code {0}")]
    // UnknownStatusCode(StatusCode),
    #[error("Couldnt verify Jwt Signature with Mojang's key")]
    InvalidJwtSignature,
    #[error("Mc access token doesn't correspond to MC account")]
    InvalidMCAccessToken,
    #[error("Userhash doesnt match")]
    UserhashDoesntMatch,
    //     #[error("Microsoft Profile doesnt own game")]
    // NoOwnership,
}

#[derive(Debug)]
pub struct Profile {
    mc_access_token: String,
    mc_profile: ProfileResponse,
}

const MOJANG_PUBLIC_KEY: &str = "-----BEGIN PUBLIC KEY-----
MIICIjANBgkqhkiG9w0BAQEFAAOCAg8AMIICCgKCAgEAtz7jy4jRH3psj5AbVS6W
NHjniqlr/f5JDly2M8OKGK81nPEq765tJuSILOWrC3KQRvHJIhf84+ekMGH7iGlO
4DPGDVb6hBGoMMBhCq2jkBjuJ7fVi3oOxy5EsA/IQqa69e55ugM+GJKUndLyHeNn
X6RzRzDT4tX/i68WJikwL8rR8Jq49aVJlIEFT6F+1rDQdU2qcpfT04CBYLM5gMxE
fWRl6u1PNQixz8vSOv8pA6hB2DU8Y08VvbK7X2ls+BiS3wqqj3nyVWqoxrwVKiXR
kIqIyIAedYDFSaIq5vbmnVtIonWQPeug4/0spLQoWnTUpXRZe2/+uAKN1RY9mmaB
pRFV/Osz3PDOoICGb5AZ0asLFf/qEvGJ+di6Ltt8/aaoBuVw+7fnTw2BhkhSq1S/
va6LxHZGXE9wsLj4CN8mZXHfwVD9QG0VNQTUgEGZ4ngf7+0u30p7mPt5sYy3H+Fm
sWXqFZn55pecmrgNLqtETPWMNpWc2fJu/qqnxE9o2tBGy/MqJiw3iLYxf7U+4le4
jM49AUKrO16bD1rdFwyVuNaTefObKjEMTX9gyVUF6o7oDEItp5NHxFm3CqnQRmch
HsMs+NxEnN4E9a8PDB23b4yjKOQ9VHDxBxuaZJU60GBCIOF9tslb7OAkheSJx5Xy
EYblHbogFGPRFU++NrSQRX0CAwEAAQ==
-----END PUBLIC KEY-----";

const XBL_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_AUTH_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const CHECK_OWNERSHIP_URL: &str = "https://api.minecraftservices.com/entitlements/mcstore";
const MINECRAFT_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

const JOIN_URL: &str = "https://sessionserver.mojang.com/session/minecraft/join";

impl Profile {
    /// Initialize instance of the struct by logging in
    pub async fn login(oauth2_access_token: &str) -> Result<Self, AuthError> {
        let (xbl_token, userhash1) = Self::xbl_auth(oauth2_access_token).await?;
        let (xsts_token, userhash2) = Self::xsts_auth(xbl_token).await?;

        if userhash1 != userhash2 {
            return Err(AuthError::UserhashDoesntMatch);
        }

        let mc_access_token = Self::minecraft_auth(userhash1, xsts_token).await?;
        // we are forced to ignore this and not error, because even if the user doesnt
        //own the game he could have Xbox gamepass, which doesnt show up as ownership
        let owns_game = Self::check_ownership(&mc_access_token).await?;
        let mc_profile = Self::get_minecraft_profile(&mc_access_token).await?;

        Ok(Profile {
            mc_access_token,
            mc_profile,
        })
    }

    pub async fn join_server(
        &self,
        server_id: &str,
        shared_secret: &[u8],
        public_key: &[u8],
    ) -> Result<(), AuthError> {
        let mut hasher = Sha1::new();
        hasher.update(server_id.as_bytes());
        hasher.update(shared_secret);
        hasher.update(public_key);
        let mut hash = hasher.finalize();

        // Mojang uses a hex method which allows for
        // negatives so we have to account for that.
        let negative = (hash[0] & 0x80) == 0x80;
        if negative {
            twos_compliment(&mut hash);
        }
        let hash_str = hash
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<String>>()
            .join("");
        let hash_val = hash_str.trim_start_matches('0');
        let hash_str = if negative {
            "-".to_owned() + hash_val
        } else {
            hash_val.to_owned()
        };

        let join_msg = json!({
            "accessToken": &self.mc_access_token,
            "selectedProfile": &self.mc_profile.uuid,
            "serverId": hash_str
        });
        let join = serde_json::to_string(&join_msg).unwrap();

        // let client = reqwest::blocking::Client::new();
        let client = reqwest::Client::new();
        let res = client
            .post(JOIN_URL)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(join)
            .send()
            .await
            .map_err(|_| AuthError::FailedResponse)?;

        match res.status() {
            StatusCode::NO_CONTENT => Ok(()),
            StatusCode::FORBIDDEN => {
                let ret: Value = res.json().await.map_err(|_| AuthError::FailedParse)?;

                let error = ret.get("error").and_then(Value::as_str);
                let message = ret.get("errorMessage").and_then(Value::as_str);

                match error.unwrap() {
                    "Forbidden" => Err(AuthError::Forbidden),
                    "UserBannedException" => Err(AuthError::Banned(message.unwrap().to_owned())),
                    "InsufficientPrivilegesException" => Err(AuthError::InsufficientPrivileges),
                    _ => panic!("unknown auth response"),
                }
            }
            _ => panic!("unknown auth response"),
        }
    }

    pub async fn xbl_auth(oauth2_access_token: &str) -> Result<(String, String), AuthError> {
        let req_msg = json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={}",oauth2_access_token) // your access token from the previous step here, make sure that it is prefixed with `d=`
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        });
        let req = serde_json::to_string(&req_msg).unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(XBL_AUTH_URL)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "application/json")
            .body(req)
            .send()
            .await
            .map_err(|_| AuthError::FailedResponse)?;

        let ret: Value = res.json().await.map_err(|_| AuthError::FailedParse)?;

        let uhs = ret
            .pointer("/DisplayClaims/xui/0/uhs")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned();
        let token = ret.get("Token").and_then(Value::as_str).unwrap().to_owned();

        Ok((token, uhs))
    }

    pub async fn xsts_auth(xbl_token: String) -> Result<(String, String), AuthError> {
        let req_msg = json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [
                    xbl_token
                ]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        });
        let req = serde_json::to_string(&req_msg).unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(XSTS_URL)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "application/json")
            .body(req)
            .send()
            .await
            .map_err(|_| AuthError::FailedResponse)?;

        let status = res.status();
        let ret: Value = res.json().await.map_err(|_| AuthError::FailedParse)?;

        match status {
            StatusCode::OK => {
                let xsts_token = ret.get("Token").and_then(Value::as_str).unwrap().to_owned();
                let uhs = ret
                    .pointer("DisplayClaims/xui/0/uhs")
                    .and_then(Value::as_str)
                    .unwrap()
                    .to_owned();

                Ok((xsts_token, uhs))
            }
            StatusCode::FORBIDDEN => {
                let x_err = ret.get("XErr").and_then(Value::as_i64).unwrap().to_owned();
                Err(AuthError::XSTSFailure(format!(
                    "Xerr error code: {}",
                    x_err
                )))
            }
            _ => panic!(),
        }
    }

    pub async fn minecraft_auth(userhash: String, xsts_token: String) -> Result<String, AuthError> {
        let req_msg = json!({
            "identityToken": format!("XBL3.0 x={userhash};{xsts_token}")
        });
        let req = serde_json::to_string(&req_msg).unwrap();

        let client = reqwest::Client::new();
        let res = client
            .post(XSTS_URL)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::ACCEPT, "application/json")
            .body(req)
            .send()
            .await
            .map_err(|_| AuthError::FailedResponse)?;

        let ret: Value = res.json().await.map_err(|_| AuthError::FailedParse)?;

        let mc_access_token = ret
            .get("access_token")
            .and_then(Value::as_str)
            .unwrap()
            .to_owned();

        Ok(mc_access_token)
    }

    pub async fn check_ownership(minecraft_access_token: &String) -> Result<bool, AuthError> {
        let client = reqwest::Client::new();
        let res = client
            .get(CHECK_OWNERSHIP_URL)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer: {}", minecraft_access_token),
            )
            .send()
            .await
            .map_err(|_| AuthError::FailedResponse)?;

        let ret: Value = res.json().await.map_err(|_| AuthError::FailedParse)?;

        // If the account doesn't own the game, the items array will be empty.
        // Note that Xbox Game Pass users don't technically own the game,
        // and therefore will not show any ownership here, but will indeed
        // have a Minecraft profile attached to their account.
        if ret.pointer("items/0").is_none() {
            return Ok(false);
        }

        // i dont know why im even unwrapping these, just need to know they exist
        let jwt1 = ret.pointer("items/0").and_then(Value::as_str).unwrap();
        let jwt2 = ret.pointer("items/1").and_then(Value::as_str).unwrap();
        let jwt3 = ret.get("signature").and_then(Value::as_str).unwrap();

        // checking one signature is enough to know if its forged or not
        let dec = jsonwebtoken::decode::<Value>(
            jwt1,
            &DecodingKey::from_rsa_pem(MOJANG_PUBLIC_KEY.as_bytes()).unwrap(),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|_| AuthError::InvalidJwtSignature)?;

        Ok(true)
    }

    pub async fn get_minecraft_profile(
        minecraft_access_token: &String,
    ) -> Result<ProfileResponse, AuthError> {
        let client = reqwest::Client::new();
        let res = client
            .get(MINECRAFT_PROFILE_URL)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer: {}", minecraft_access_token),
            )
            .send()
            .await
            .map_err(|_| AuthError::FailedResponse)?;

        let status = res.status();
        let ret: Value = res.json().await.map_err(|_| AuthError::FailedParse)?;

        match status {
            StatusCode::OK => {
                let profile: ProfileResponse =
                    serde_json::from_value(ret).map_err(|_| AuthError::FailedParse)?;
                Ok(profile)
            }
            StatusCode::NOT_FOUND => Err(AuthError::InvalidMCAccessToken),
            _ => panic!(),
        }
    }

    pub async fn from_mc_token(minecraft_access_token: &String) -> Result<Self, AuthError> {
        Ok(Profile {
            mc_access_token: minecraft_access_token.to_owned(),
            mc_profile: Profile::get_minecraft_profile(minecraft_access_token).await?,
        })
    }
}

fn twos_compliment(data: &mut [u8]) {
    let mut carry = true;
    for i in (0..data.len()).rev() {
        data[i] = !data[i];
        if carry {
            carry = data[i] == 0xFF;
            data[i] = data[i].wrapping_add(1);
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ProfileResponse {
    /// the real uuid of the account WITHOUT DASHES
    #[serde(rename = "id")]
    uuid: String,
    /// the mc user name of the account
    #[serde(rename = "name")]
    username: String,
    skins: Vec<Skin>,
    capes: Vec<Cape>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Skin {
    id: String,
    state: String,
    url: String,
    variant: String,
    alias: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Cape {
    id: String,
    state: String,
    url: String,
    alias: String,
}
