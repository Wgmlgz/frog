//! Xiaomi account access is used only during setup to obtain a miIO token.
//! Routine lamp control stays on the local network.
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use md5::{Digest as _, Md5};
use rc4::{consts::U32, KeyInit, Rc4, StreamCipher};
use reqwest::{
    blocking::{Client, Response},
    cookie::{CookieStore, Jar},
    redirect::Policy,
    Url,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha1::Sha1;
use sha2::Sha256;
use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const ACCOUNT: &str = "https://account.xiaomi.com";
const REGIONS: &[&str] = &["cn", "de", "us", "ru", "tw", "sg", "in", "i2"];

#[derive(Clone)]
pub(crate) struct CloudDevice {
    pub did: String,
    pub name: String,
    pub model: String,
    pub token: String,
    pub local_ip: String,
}

#[derive(Serialize)]
pub(crate) struct CloudDeviceView {
    did: String,
    name: String,
    model: String,
    local_ip: String,
    has_token: bool,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum LoginStep {
    Captcha { image: String, wrong: bool },
    EmailCode,
    Ready { devices: Vec<CloudDeviceView> },
}

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct SavedXiaomiAccount {
    pub username: String,
    pub user_id: String,
    pub ssecurity: String,
    pub service_token: String,
    pub region: String,
}

#[derive(Serialize)]
pub(crate) struct XiaomiAccountView {
    user_id: String,
    username: String,
    region: String,
}

impl From<&SavedXiaomiAccount> for XiaomiAccountView {
    fn from(account: &SavedXiaomiAccount) -> Self {
        Self {
            user_id: account.user_id.clone(),
            username: account.username.clone(),
            region: account.region.clone(),
        }
    }
}

pub(crate) struct CloudSession {
    client: Client,
    no_redirect: Client,
    jar: Arc<Jar>,
    username: String,
    password: String,
    sign: String,
    user_id: String,
    ssecurity: String,
    service_token: String,
    notification_url: String,
    region: String,
    pub devices: Vec<CloudDevice>,
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn json_response(response: Response) -> Result<Value, String> {
    if !response.status().is_success() {
        return Err(format!(
            "Xiaomi account returned HTTP {}",
            response.status()
        ));
    }
    let body = response.text().map_err(|e| e.to_string())?;
    serde_json::from_str(
        body.trim_start()
            .strip_prefix("&&&START&&&")
            .unwrap_or(body.trim_start()),
    )
    .map_err(|_| "Xiaomi account returned an unreadable response".into())
}

fn absolute_account_url(raw: &str) -> Result<Url, String> {
    let url = if raw.starts_with('/') {
        format!("{ACCOUNT}{raw}")
    } else {
        raw.to_owned()
    };
    let url = Url::parse(&url).map_err(|_| "Invalid Xiaomi account URL")?;
    if url.scheme() != "https" || url.host_str() != Some("account.xiaomi.com") {
        return Err("Xiaomi account returned an unexpected verification URL".into());
    }
    Ok(url)
}

fn is_xiaomi_url(url: &Url) -> bool {
    url.scheme() == "https"
        && matches!(
            url.host_str(),
            Some("account.xiaomi.com" | "sts.api.io.mi.com")
        )
}

fn string(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_i64().map(|v| v.to_string()))
        .unwrap_or_default()
}

impl CloudSession {
    pub fn from_saved(account: &SavedXiaomiAccount) -> Result<Self, String> {
        if account.user_id.is_empty()
            || account.ssecurity.is_empty()
            || account.service_token.is_empty()
        {
            return Err("Saved Xiaomi session is incomplete".into());
        }
        let mut session = Self::new(
            account.username.clone(),
            String::from("saved-session"),
            account.region.clone(),
        )?;
        session.password.clear();
        session.user_id = account.user_id.clone();
        session.ssecurity = account.ssecurity.clone();
        session.service_token = account.service_token.clone();
        Ok(session)
    }

    pub fn saved_account(&self) -> Option<SavedXiaomiAccount> {
        if self.user_id.is_empty() || self.ssecurity.is_empty() || self.service_token.is_empty() {
            return None;
        }
        Some(SavedXiaomiAccount {
            username: self.username.clone(),
            user_id: self.user_id.clone(),
            ssecurity: self.ssecurity.clone(),
            service_token: self.service_token.clone(),
            region: self.region.clone(),
        })
    }

    pub fn refresh(&mut self) -> Result<LoginStep, String> {
        self.ready()
    }

    pub fn new(username: String, password: String, region: String) -> Result<Self, String> {
        if username.trim().is_empty() || password.is_empty() {
            return Err("Enter your Xiaomi account and password".into());
        }
        if region != "all" && !REGIONS.contains(&region.as_str()) {
            return Err("Unsupported Xiaomi region".into());
        }
        let jar = Arc::new(Jar::default());
        let account_url = Url::parse(ACCOUNT).unwrap();
        let digest = Sha256::digest(format!("frog-device-id\0{username}").as_bytes());
        let device_id: String = digest[..6]
            .iter()
            .map(|byte| (b'a' + byte % 26) as char)
            .collect();
        jar.add_cookie_str(
            &format!("deviceId={device_id}; Domain=.xiaomi.com; Path=/"),
            &account_url,
        );
        jar.add_cookie_str(
            "sdkVersion=accountsdk-18.8.15; Domain=.xiaomi.com; Path=/",
            &account_url,
        );
        jar.add_cookie_str(
            &format!("userId={username}; Domain=.xiaomi.com; Path=/"),
            &account_url,
        );
        let agent = format!(
            "Android-7.1.1-1.0.0-ONEPLUS A3010-136-{} APP/xiaomi.smarthome APP/1.0",
            device_id.to_uppercase()
        );
        let client = Client::builder()
            .cookie_provider(jar.clone())
            .user_agent(agent.clone())
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        let no_redirect = Client::builder()
            .cookie_provider(jar.clone())
            .user_agent(agent)
            .redirect(Policy::none())
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Self {
            client,
            no_redirect,
            jar,
            username,
            password,
            sign: String::new(),
            user_id: String::new(),
            ssecurity: String::new(),
            service_token: String::new(),
            notification_url: String::new(),
            region,
            devices: Vec::new(),
        })
    }

    pub fn begin(&mut self) -> Result<LoginStep, String> {
        let body = json_response(
            self.client
                .get(format!(
                    "{ACCOUNT}/pass/serviceLogin?sid=xiaomiio&_json=true"
                ))
                .send()
                .map_err(|e| format!("Xiaomi login could not start: {e}"))?,
        )?;
        self.sign = string(&body["_sign"]);
        if self.sign.is_empty() {
            return Err("Xiaomi login did not provide a sign token".into());
        }
        self.authenticate("")
    }

    pub fn captcha(&mut self, code: &str) -> Result<LoginStep, String> {
        if code.trim().is_empty() {
            return Err("Enter the CAPTCHA characters".into());
        }
        self.authenticate(code.trim())
    }

    fn authenticate(&mut self, captcha: &str) -> Result<LoginStep, String> {
        let hash = hex::encode_upper(Md5::digest(self.password.as_bytes()));
        let mut form = vec![
            ("sid", "xiaomiio"),
            ("hash", hash.as_str()),
            ("callback", "https://sts.api.io.mi.com/sts"),
            ("qs", "%3Fsid%3Dxiaomiio%26_json%3Dtrue"),
            ("user", self.username.as_str()),
            ("_sign", self.sign.as_str()),
            ("_json", "true"),
        ];
        if !captcha.is_empty() {
            form.push(("captCode", captcha));
        }
        let body = json_response(
            self.no_redirect
                .post(format!("{ACCOUNT}/pass/serviceLoginAuth2"))
                .form(&form)
                .send()
                .map_err(|e| format!("Xiaomi login failed: {e}"))?,
        )?;
        if let Some(security) = body["ssecurity"].as_str().filter(|value| !value.is_empty()) {
            self.ssecurity = security.to_owned();
            self.user_id = string(&body["userId"]);
            self.finish_login(
                body["location"]
                    .as_str()
                    .ok_or("Xiaomi login returned no service URL")?,
            )?;
            self.password.clear();
            return self.ready();
        }
        if let Some(raw) = body["captchaUrl"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            let url = absolute_account_url(raw)?;
            let response = self
                .client
                .get(url)
                .send()
                .map_err(|e| format!("Could not fetch CAPTCHA: {e}"))?;
            let mime = response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .filter(|value| value.starts_with("image/"))
                .unwrap_or("image/png")
                .to_owned();
            let bytes = response.bytes().map_err(|e| e.to_string())?;
            if bytes.len() > 1_000_000 {
                return Err("CAPTCHA image is too large".into());
            }
            return Ok(LoginStep::Captcha {
                image: format!("data:{mime};base64,{}", BASE64.encode(bytes)),
                wrong: !captcha.is_empty(),
            });
        }
        if let Some(raw) = body["notificationUrl"]
            .as_str()
            .filter(|value| !value.is_empty())
        {
            self.notification_url = absolute_account_url(raw)?.to_string();
            self.send_email_code()?;
            return Ok(LoginStep::EmailCode);
        }
        Err(format!(
            "Xiaomi rejected the login (code {})",
            string(&body["code"])
        ))
    }

    fn cookie(&self, url: &str, name: &str) -> String {
        let Ok(url) = Url::parse(url) else {
            return String::new();
        };
        self.jar
            .cookies(&url)
            .and_then(|value| value.to_str().ok().map(str::to_owned))
            .and_then(|cookies| {
                cookies.split(';').find_map(|part| {
                    part.trim()
                        .strip_prefix(&format!("{name}="))
                        .map(str::to_owned)
                })
            })
            .unwrap_or_default()
    }

    fn send_email_code(&self) -> Result<(), String> {
        let url = Url::parse(&self.notification_url).map_err(|e| e.to_string())?;
        let context = url
            .query_pairs()
            .find(|(key, _)| key == "context")
            .map(|(_, value)| value.into_owned())
            .ok_or("Xiaomi verification did not include a context")?;
        self.client.get(url).send().map_err(|e| e.to_string())?;
        self.client
            .get(format!("{ACCOUNT}/identity/list"))
            .query(&[
                ("sid", "xiaomiio"),
                ("context", context.as_str()),
                ("_locale", "en_US"),
            ])
            .send()
            .map_err(|e| e.to_string())?;
        let body = json_response(
            self.client
                .post(format!("{ACCOUNT}/identity/auth/sendEmailTicket"))
                .query(&[
                    ("_dc", now_millis().to_string()),
                    ("sid", "xiaomiio".into()),
                    ("context", context),
                    ("mask", "0".into()),
                    ("_locale", "en_US".into()),
                ])
                .form(&[
                    ("retry", "0"),
                    ("icode", ""),
                    ("_json", "true"),
                    ("ick", self.cookie(ACCOUNT, "ick").as_str()),
                ])
                .send()
                .map_err(|e| e.to_string())?,
        )?;
        if !body["code"].is_null() && body["code"] != 0 {
            return Err("Xiaomi did not send an email verification code".into());
        }
        Ok(())
    }

    pub fn email_code(&mut self, code: &str) -> Result<LoginStep, String> {
        if code.trim().is_empty() {
            return Err("Enter the email verification code".into());
        }
        let url = Url::parse(&self.notification_url).map_err(|e| e.to_string())?;
        let context = url
            .query_pairs()
            .find(|(key, _)| key == "context")
            .map(|(_, value)| value.into_owned())
            .ok_or("Xiaomi verification did not include a context")?;
        let response = self
            .no_redirect
            .post(format!("{ACCOUNT}/identity/auth/verifyEmail"))
            .query(&[
                ("_flag", "8"),
                ("_json", "true"),
                ("sid", "xiaomiio"),
                ("context", context.as_str()),
                ("mask", "0"),
                ("_locale", "en_US"),
            ])
            .form(&[
                ("_flag", "8"),
                ("ticket", code.trim()),
                ("trust", "true"),
                ("_json", "true"),
                ("ick", self.cookie(ACCOUNT, "ick").as_str()),
            ])
            .send()
            .map_err(|e| format!("Xiaomi verification failed: {e}"))?;
        let location_header = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = response.text().map_err(|e| e.to_string())?;
        let parsed: Value = serde_json::from_str(
            body.trim()
                .strip_prefix("&&&START&&&")
                .unwrap_or(body.trim()),
        )
        .unwrap_or(Value::Null);
        if !parsed["code"].is_null() && parsed["code"] != 0 {
            return Err("Xiaomi rejected the email code".into());
        }
        let mut next = parsed["location"]
            .as_str()
            .map(str::to_owned)
            .or(location_header)
            .unwrap_or_else(|| {
                format!(
                    "{ACCOUNT}/identity/result/check?sid=xiaomiio&context={context}&_locale=en_US"
                )
            });
        next = absolute_account_url(&next)?.to_string();
        if next.contains("identity/result/check") {
            let check = self
                .no_redirect
                .get(&next)
                .send()
                .map_err(|e| e.to_string())?;
            next = check
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or("Xiaomi verification returned no continuation")?
                .to_owned();
        }
        let end = absolute_account_url(&next)?;
        let mut response = self
            .no_redirect
            .get(end.clone())
            .send()
            .map_err(|e| e.to_string())?;
        if response.status().is_success() && response.headers().get("extension-pragma").is_none() {
            response = self
                .no_redirect
                .get(end)
                .send()
                .map_err(|e| e.to_string())?;
        }
        let pragma = response
            .headers()
            .get("extension-pragma")
            .and_then(|value| value.to_str().ok())
            .ok_or("Xiaomi verification returned no session key")?;
        let extension: Value =
            serde_json::from_str(pragma).map_err(|_| "Invalid Xiaomi session key")?;
        self.ssecurity = extension["ssecurity"]
            .as_str()
            .ok_or("Missing Xiaomi session key")?
            .to_owned();
        let sts = response
            .headers()
            .get(reqwest::header::LOCATION)
            .and_then(|value| value.to_str().ok())
            .ok_or("Xiaomi verification returned no service URL")?;
        self.finish_login(sts)?;
        self.user_id = self.cookie(ACCOUNT, "userId");
        if self.user_id.is_empty() {
            self.user_id = self.cookie("https://sts.api.io.mi.com", "userId");
        }
        self.password.clear();
        self.ready()
    }

    fn finish_login(&mut self, raw: &str) -> Result<(), String> {
        let url = if raw.starts_with('/') {
            Url::parse(&format!("{ACCOUNT}{raw}"))
        } else {
            Url::parse(raw)
        }
        .map_err(|_| "Invalid Xiaomi service URL")?;
        if !is_xiaomi_url(&url) {
            return Err("Xiaomi returned an unexpected service URL".into());
        }
        self.client
            .get(url)
            .send()
            .map_err(|e| format!("Xiaomi service login failed: {e}"))?;
        self.service_token = self.cookie("https://sts.api.io.mi.com", "serviceToken");
        if self.service_token.is_empty() {
            self.service_token = self.cookie(ACCOUNT, "serviceToken");
        }
        if self.service_token.is_empty() {
            return Err("Xiaomi login returned no service token".into());
        }
        Ok(())
    }

    fn ready(&mut self) -> Result<LoginStep, String> {
        self.devices = self.fetch_devices()?;
        Ok(LoginStep::Ready {
            devices: self
                .devices
                .iter()
                .map(|d| CloudDeviceView {
                    did: d.did.clone(),
                    name: d.name.clone(),
                    model: d.model.clone(),
                    local_ip: d.local_ip.clone(),
                    has_token: d.token.len() == 32,
                })
                .collect(),
        })
    }

    fn fetch_devices(&self) -> Result<Vec<CloudDevice>, String> {
        let regions: Vec<_> = if self.region == "all" {
            REGIONS.to_vec()
        } else {
            vec![self.region.as_str()]
        };
        let mut devices = Vec::new();
        let mut first_error = None;
        for region in regions {
            match self.region_devices(region) {
                Ok(found) => {
                    for device in found {
                        if !devices
                            .iter()
                            .any(|existing: &CloudDevice| existing.did == device.did)
                        {
                            devices.push(device);
                        }
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                }
            }
        }
        if devices.is_empty() {
            if let Some(error) = first_error {
                return Err(error);
            }
        }
        Ok(devices)
    }

    fn region_devices(&self, region: &str) -> Result<Vec<CloudDevice>, String> {
        let host = if region == "cn" {
            "api.io.mi.com".to_owned()
        } else {
            format!("{region}.api.io.mi.com")
        };
        let url = format!("https://{host}/app/home/device_list");
        let data = self.encrypted_call(&url, r#"{"getVirtualModel":false,"getHuamiDevices":0}"#)?;
        if data["code"] != 0 {
            return Err(format!("Xiaomi device list failed in region {region}"));
        }
        let mut devices = Vec::new();
        if let Some(list) = data["result"]["list"].as_array() {
            for item in list {
                let did = string(&item["did"]);
                if did.is_empty() {
                    continue;
                }
                devices.push(CloudDevice {
                    did,
                    name: string(&item["name"]),
                    model: string(&item["model"]),
                    token: string(&item["token"]),
                    local_ip: string(&item["localip"]),
                });
            }
        }
        Ok(devices)
    }

    fn encrypted_call(&self, url: &str, data: &str) -> Result<Value, String> {
        let mut nonce = [0u8; 12];
        nonce[..8].copy_from_slice(&Uuid::new_v4().as_bytes()[..8]);
        nonce[8..].copy_from_slice(&((now_millis() / 60_000) as u32).to_be_bytes());
        let nonce = BASE64.encode(nonce);
        let mut key_material = BASE64
            .decode(&self.ssecurity)
            .map_err(|_| "Invalid Xiaomi session key")?;
        key_material.extend(BASE64.decode(&nonce).map_err(|_| "Invalid Xiaomi nonce")?);
        let signed = BASE64.encode(Sha256::digest(key_material));
        let parsed = Url::parse(url).map_err(|e| e.to_string())?;
        let path = parsed.path().strip_prefix("/app").unwrap_or(parsed.path());
        let mut params = BTreeMap::from([("data".to_owned(), data.to_owned())]);
        let hash = signature(path, &params, &signed);
        params.insert("rc4_hash__".into(), hash);
        for value in params.values_mut() {
            *value = BASE64.encode(rc4_crypt(&signed, value.as_bytes())?);
        }
        let sign = signature(path, &params, &signed);
        params.insert("signature".into(), sign);
        params.insert("ssecurity".into(), self.ssecurity.clone());
        params.insert("_nonce".into(), nonce);
        let response = self.client.post(url).form(&params)
            .header("Accept-Encoding", "identity")
            .header("x-xiaomi-protocal-flag-cli", "PROTOCAL-HTTP2")
            .header("MIOT-ENCRYPT-ALGORITHM", "ENCRYPT-RC4")
            .header(reqwest::header::COOKIE, format!("userId={}; serviceToken={}; yetAnotherServiceToken={}; locale=en_GB; channel=MI_APP_STORE",
                self.user_id, self.service_token, self.service_token))
            .send().map_err(|e| format!("Xiaomi device lookup failed: {e}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Xiaomi device lookup returned HTTP {}",
                response.status()
            ));
        }
        let text = response.text().map_err(|e| e.to_string())?;
        let encrypted = BASE64
            .decode(text.trim())
            .map_err(|_| "Xiaomi device response was not encrypted as expected")?;
        serde_json::from_slice(&rc4_crypt(&signed, &encrypted)?)
            .map_err(|_| "Could not decode Xiaomi device list".into())
    }
}

fn signature(path: &str, fields: &BTreeMap<String, String>, signed: &str) -> String {
    let mut parts = vec!["POST".to_owned(), path.to_owned()];
    parts.extend(fields.iter().map(|(key, value)| format!("{key}={value}")));
    parts.push(signed.to_owned());
    BASE64.encode(Sha1::digest(parts.join("&").as_bytes()))
}

fn rc4_crypt(signed: &str, input: &[u8]) -> Result<Vec<u8>, String> {
    let key = BASE64
        .decode(signed)
        .map_err(|_| "Invalid Xiaomi encryption key")?;
    let mut cipher =
        Rc4::<U32>::new_from_slice(&key).map_err(|_| "Invalid Xiaomi encryption key")?;
    let mut drop = [0u8; 1024];
    cipher.apply_keystream(&mut drop);
    let mut output = input.to_vec();
    cipher.apply_keystream(&mut output);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn xiaomi_crypto_vectors() {
        let security = BASE64.decode("MDEyMzQ1Njc4OWFiY2RlZg==").unwrap();
        let nonce = BASE64.decode("AAECAwQFBgcBuoFA").unwrap();
        let signed = BASE64.encode(Sha256::digest([security, nonce].concat()));
        assert_eq!(signed, "kyT2Getw70/0rz6eein9HXnT6/gihg6piyxhhlIe7WY=");
        let data = r#"{"getVirtualModel":false,"getHuamiDevices":0}"#;
        let fields = BTreeMap::from([("data".into(), data.into())]);
        assert_eq!(
            signature("/home/device_list", &fields, &signed),
            "o+0TnCOLuJfLXO7FqiwnUeS6egA="
        );
        let encrypted = rc4_crypt(&signed, data.as_bytes()).unwrap();
        assert_eq!(
            BASE64.encode(&encrypted),
            "CLaumw7Uk691UbkVoIpz9yQ6OcrNupSz2YP7vxWJREWtVSxhms7XW3Drgz7w"
        );
        assert_eq!(rc4_crypt(&signed, &encrypted).unwrap(), data.as_bytes());
    }
}
