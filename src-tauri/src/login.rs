use crate::http;
use reqwest::{header, Client};
use serde_json::{json, Value};
use std::time::Duration;

const PASSPORT: &str = "https://passport.bilibili.com";
const FINGER: &str = "https://api.bilibili.com/x/frontend/finger/spi";

fn login_client() -> Result<Client, String> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::ORIGIN,
        header::HeaderValue::from_static("https://www.bilibili.com"),
    );
    headers.insert(
        header::REFERER,
        header::HeaderValue::from_static("https://www.bilibili.com/"),
    );
    Client::builder()
        .default_headers(headers)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|_| "无法初始化网络连接".into())
}

fn query_param(url: &str, key: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .query_pairs()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.into_owned())
        })
        .unwrap_or_default()
}

async fn buvid() -> (String, String) {
    let client = match login_client() {
        Ok(client) => client,
        Err(_) => return (String::new(), String::new()),
    };
    let value = match client
        .get(FINGER)
        .header(header::USER_AGENT, http::user_agent())
        .send()
        .await
    {
        Ok(response) => match response.json::<Value>().await {
            Ok(value) => value,
            Err(_) => return (String::new(), String::new()),
        },
        Err(_) => return (String::new(), String::new()),
    };
    (
        http::string(&value["data"]["b_3"]),
        http::string(&value["data"]["b_4"]),
    )
}

#[tauri::command]
pub async fn bili_qrcode_generate() -> Result<Value, String> {
    let client = login_client()?;
    let response = client
        .get(format!("{PASSPORT}/x/passport-login/web/qrcode/generate"))
        .header(header::USER_AGENT, http::user_agent())
        .send()
        .await
        .map_err(|_| "无法获取登录二维码，请检查网络连接")?;
    let value: Value = response
        .json()
        .await
        .map_err(|_| "登录接口返回了异常数据")?;
    if value["code"].as_i64() != Some(0) {
        return Err(format!("获取登录二维码失败（{}）", http::number(&value["code"])));
    }
    let url = http::string(&value["data"]["url"]);
    let qrcode_key = http::string(&value["data"]["qrcode_key"]);
    if url.is_empty() || qrcode_key.is_empty() {
        return Err("登录接口未返回二维码信息".into());
    }
    Ok(json!({ "url": url, "qrcodeKey": qrcode_key }))
}

#[tauri::command]
pub async fn bili_qrcode_poll(qrcode_key: String) -> Result<Value, String> {
    let qrcode_key = qrcode_key.trim();
    if qrcode_key.is_empty() {
        return Err("二维码会话无效，请重新获取".into());
    }
    let client = login_client()?;
    let response = client
        .get(format!("{PASSPORT}/x/passport-login/web/qrcode/poll"))
        .query(&[("qrcode_key", qrcode_key)])
        .header(header::USER_AGENT, http::user_agent())
        .send()
        .await
        .map_err(|_| "查询扫码状态失败，请检查网络连接")?;
    let value: Value = response
        .json()
        .await
        .map_err(|_| "登录接口返回了异常数据")?;
    if value["code"].as_i64() != Some(0) {
        return Err(format!("查询扫码状态失败（{}）", http::number(&value["code"])));
    }
    let data = &value["data"];
    let code = http::number(&data["code"]);
    let message = http::string(&data["message"]);
    let status = match code {
        86101 => "waiting",
        86090 => "scanned",
        86038 => "expired",
        0 => "confirmed",
        _ => {
            return Err(if message.is_empty() {
                format!("扫码状态异常（{code}）")
            } else {
                message
            })
        }
    };
    let mut result = json!({ "status": status, "message": message, "cookie": "", "uid": "" });
    if status == "confirmed" {
        let url = http::string(&data["url"]);
        let sessdata = query_param(&url, "SESSDATA");
        let bili_jct = query_param(&url, "bili_jct");
        if sessdata.is_empty() || bili_jct.is_empty() {
            return Err("扫码成功，但未获取到登录凭证，请重新扫码".into());
        }
        let dedeuserid = query_param(&url, "DedeUserID");
        let dedeuserid_ck = query_param(&url, "DedeUserID__ckMd5");
        let (buvid3, buvid4) = buvid().await;
        let mut cookie = format!("SESSDATA={sessdata}; bili_jct={bili_jct}");
        if !dedeuserid.is_empty() {
            cookie.push_str(&format!("; DedeUserID={dedeuserid}"));
        }
        if !dedeuserid_ck.is_empty() {
            cookie.push_str(&format!("; DedeUserID__ckMd5={dedeuserid_ck}"));
        }
        if !buvid3.is_empty() {
            cookie.push_str(&format!("; buvid3={buvid3}"));
        }
        if !buvid4.is_empty() {
            cookie.push_str(&format!("; buvid4={buvid4}"));
        }
        result["cookie"] = json!(cookie);
        result["uid"] = json!(dedeuserid);
    }
    Ok(result)
}
