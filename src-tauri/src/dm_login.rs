use crate::{clock, http};
use reqwest::{header, redirect::Policy, Client};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;

const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/113.0.0.0 Safari/537.36";
const LOGIN_REFERER: &str =
    "https://ipassport.damai.cn/mini_login.htm?lang=zh_cn&appName=damai&appEntrance=default";
const QR_GENERATE: &str = "https://ipassport.damai.cn/newlogin/qrcode/generate.do";
const QR_QUERY: &str = "https://ipassport.damai.cn/newlogin/qrcode/query.do";
const RETURN_URL: &str =
    "https://passport.damai.cn/dologin.htm?redirectUrl=https%253A%252F%252Fwww.damai.cn%252F&platform=106002";
const DAMAI_HOME: &str = "https://m.damai.cn/";
const MYINFO: &str =
    "https://passport.damai.cn/accountinfo/myinfo?spm=a2oeg.home.nick.duserinfo.591b23e1MY5Dhp";
const MTOP_BROADCAST: &str =
    "https://mtop.damai.cn/h5/mtop.damai.wireless.search.broadcast.list/1.0/";

fn random_alnum(len: usize) -> String {
    let mut out = String::new();
    while out.len() < len {
        out.push_str(&uuid::Uuid::new_v4().simple().to_string());
    }
    out.truncate(len);
    out
}

fn login_client() -> Result<Client, String> {
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::ACCEPT,
        header::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::REFERER,
        header::HeaderValue::from_static(LOGIN_REFERER),
    );
    headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_static(BROWSER_UA),
    );
    Client::builder()
        .default_headers(headers)
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "无法初始化网络连接".to_string())
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

fn collect_cookies(response: &reqwest::Response, jar: &mut HashMap<String, String>) {
    for value in response.headers().get_all(header::SET_COOKIE) {
        let Ok(line) = value.to_str() else { continue };
        for part in line.split(';') {
            let part = part.trim();
            if let Some((name, value)) = part.split_once('=') {
                if !name.is_empty() {
                    jar.insert(name.to_string(), value.to_string());
                }
            }
        }
    }
}

fn cookie_header(jar: &HashMap<String, String>) -> String {
    jar.iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

#[tauri::command]
pub async fn dm_qrcode_generate() -> Result<Value, String> {
    let client = login_client()?;
    let csrf = random_alnum(21);
    let umid = random_alnum(40);
    let hsiz = format!("{:x}", md5::compute(random_alnum(10)));
    let response = client
        .post(QR_GENERATE)
        .query(&[
            ("appName", "damai"),
            ("fromSite", "18"),
            ("appEntrance", "damai"),
            ("_csrf_token", csrf.as_str()),
            ("umidToken", umid.as_str()),
            ("isMobile", "false"),
            ("lang", "zh_CN"),
            ("returnUrl", RETURN_URL),
            ("hsiz", hsiz.as_str()),
            ("bizParams", ""),
            ("umidTag", "SERVER"),
            ("_bx-v", "2.5.0"),
        ])
        .send()
        .await
        .map_err(|_| "无法获取登录二维码，请检查网络连接")?;
    let value: Value = response
        .json()
        .await
        .map_err(|_| "登录接口返回了异常数据")?;
    if value["content"]["success"] != true {
        return Err("获取登录二维码失败，请稍后重试".to_string());
    }
    let data = &value["content"]["data"];
    let ck = http::string(&data["ck"]);
    let code_content = http::string(&data["codeContent"]);
    let t = http::number(&data["t"]).to_string();
    if ck.is_empty() || code_content.is_empty() || t == "0" {
        return Err("登录接口未返回二维码信息".to_string());
    }
    Ok(json!({ "lgToken": ck, "t": t, "qrcodeContent": code_content }))
}

#[tauri::command]
pub async fn dm_qrcode_poll(lg_token: String, t: String) -> Result<Value, String> {
    let lg_token = lg_token.trim().to_string();
    let t = t.trim().to_string();
    if lg_token.is_empty() || t.is_empty() {
        return Err("二维码会话无效，请重新获取".to_string());
    }
    let client = login_client()?;
    let csrf = random_alnum(21);
    let umid = random_alnum(40);
    let hsiz = format!("{:x}", md5::compute(random_alnum(10)));
    let device = random_alnum(24);
    let trace = random_alnum(40);
    let response = client
        .post(QR_QUERY)
        .query(&[
            ("appName", "damai"),
            ("fromSite", "18"),
            ("_bx-v", "2.5.0"),
        ])
        .form(&[
            ("t", t.as_str()),
            ("ck", lg_token.as_str()),
            ("appName", "damai"),
            ("appEntrance", "damai"),
            ("fromSite", "18"),
            ("_csrf_token", csrf.as_str()),
            ("umidToken", umid.as_str()),
            ("isMobile", "false"),
            ("lang", "zh_CN"),
            ("returnUrl", RETURN_URL),
            ("hsiz", hsiz.as_str()),
            ("bizParams", ""),
            ("umidTag", "SERVER"),
            ("navlanguage", "zh-CN"),
            ("navUserAgent", BROWSER_UA),
            ("navPlatform", "MacIntel"),
            ("deviceId", device.as_str()),
            ("pageTraceId", trace.as_str()),
        ])
        .send()
        .await
        .map_err(|_| "查询扫码状态失败，请检查网络连接")?;
    let value: Value = response
        .json()
        .await
        .map_err(|_| "登录接口返回了异常数据")?;
    if value["content"]["success"] != true {
        return Err("查询扫码状态失败，请稍后重试".to_string());
    }
    let data = &value["content"]["data"];
    let status_code = http::string(&data["qrCodeStatus"]);
    let status = match status_code.as_str() {
        "NEW" | "PUSHED" => "waiting",
        "SCANED" => "scanned",
        "CONFIRMED" => "confirmed",
        "CANCELED" => "cancelled",
        "EXPIRED" => "expired",
        "ERROR" => "error",
        other => {
            return Err(if other.is_empty() {
                "扫码状态异常，请重新获取二维码".to_string()
            } else {
                format!("扫码状态异常（{other}）")
            })
        }
    };
    Ok(json!({
        "status": status,
        "message": "",
        "cookie2": http::string(&data["cookie2"]),
        "returnUrl": http::string(&data["returnUrl"]),
        "st": http::string(&data["st"]),
    }))
}

#[tauri::command]
pub async fn dm_qrcode_exchange(
    cookie2: String,
    return_url: String,
    st: String,
) -> Result<Value, String> {
    let cookie2 = cookie2.trim().to_string();
    let return_url = return_url.trim().to_string();
    let st = st.trim().to_string();
    if cookie2.is_empty() || return_url.is_empty() || st.is_empty() {
        return Err("登录凭证无效，请重新扫码".to_string());
    }
    let client = Client::builder()
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|_| "无法初始化网络连接".to_string())?;
    let mut jar: HashMap<String, String> = HashMap::new();
    jar.insert("cookie2".to_string(), cookie2);

    let mut current = format!("{return_url}&st={st}");
    let base = reqwest::Url::parse(&current).map_err(|_| "登录跳转地址无效".to_string())?;
    for _ in 0..8 {
        let response = client
            .get(&current)
            .header(header::USER_AGENT, BROWSER_UA)
            .header(header::COOKIE, cookie_header(&jar))
            .send()
            .await
            .map_err(|_| "获取登录凭证失败，请检查网络连接")?;
        collect_cookies(&response, &mut jar);
        match response.headers().get(header::LOCATION) {
            Some(location) => {
                let location = location
                    .to_str()
                    .map_err(|_| "登录跳转地址无效".to_string())?
                    .to_string();
                current = base
                    .join(&location)
                    .map(|parsed| parsed.to_string())
                    .map_err(|_| "登录跳转地址无效".to_string())?;
            }
            None => break,
        }
    }

    let home = client
        .get(DAMAI_HOME)
        .header(header::USER_AGENT, BROWSER_UA)
        .header(header::COOKIE, cookie_header(&jar))
        .send()
        .await;
    if let Ok(response) = home {
        collect_cookies(&response, &mut jar);
    }

    let myinfo = client
        .get(MYINFO)
        .header(header::USER_AGENT, BROWSER_UA)
        .header(header::COOKIE, cookie_header(&jar))
        .send()
        .await;
    if let Ok(response) = myinfo {
        collect_cookies(&response, &mut jar);
    }

    let ts = clock::now_ms().to_string();
    let request_start = (clock::now_ms() - 1).to_string();
    let mtop = client
        .get(MTOP_BROADCAST)
        .query(&[
            ("jsv", "2.7.2"),
            ("appKey", "12574478"),
            ("type", "originaljson"),
            ("dataType", "json"),
            ("H5Request", "true"),
            ("AntiCreep", "true"),
            ("AntiFlood", "true"),
            ("t", ts.as_str()),
            ("requestStart", request_start.as_str()),
            ("api", "mtop.damai.wireless.search.broadcast.list"),
            ("v", "1.0"),
        ])
        .header(header::USER_AGENT, BROWSER_UA)
        .header(header::ORIGIN, "https://mtop.damai.cn")
        .header(header::REFERER, "https://m.damai.cn/")
        .header(header::COOKIE, cookie_header(&jar))
        .send()
        .await;
    if let Ok(response) = mtop {
        collect_cookies(&response, &mut jar);
    }

    let cookie = cookie_header(&jar);
    if http::cookie_value(&cookie, "_m_h5_tk").is_empty() {
        return Err(
            "已登录大麦，但未能获取访问凭证（_m_h5_tk），请稍后重试或改用浏览器复制 Cookie"
                .to_string(),
        );
    }
    let uid = query_param(&current, "user_num_id");
    Ok(json!({ "cookie": cookie, "uid": uid }))
}
