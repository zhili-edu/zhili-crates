use rand::Rng as _;
use rsa::{
    RsaPrivateKey, RsaPublicKey,
    pkcs1v15::{Signature, SigningKey, VerifyingKey},
    signature::{RandomizedSigner as _, SignatureEncoding as _, Verifier as _},
};
use sha2::Sha256;
use time::OffsetDateTime;

/// https://pay.weixin.qq.com/doc/v3/merchant/4012365336
pub fn get_body_auth_header(
    mchid: &str,
    key: RsaPrivateKey,
    serial: &str,
    method: http::Method,
    uri: &str,
    body: &str,
) -> String {
    let nonce = {
        let mut rng = rand::thread_rng();
        (0..30)
            .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
            .collect::<String>()
    };
    let timestamp = OffsetDateTime::now_utc().unix_timestamp();

    let sign = get_sign(key, method, uri, body, &nonce, timestamp);

    format!(
        r#"WECHATPAY2-SHA256-RSA2048 mchid="{mchid}",nonce_str="{nonce}",signature="{sign}",timestamp="{timestamp}",serial_no="{serial}"""#
    )
}

fn get_sign(
    key: RsaPrivateKey,
    method: http::Method,
    uri: &str,
    body: &str,
    nonce: &str,
    timestamp: i64,
) -> String {
    let str_to_sign = {
        let method = method.to_string();

        format!("{method}\n{uri}\n{timestamp}\n{nonce}\n{body}\n")
    };

    let sign = sign_sha256_rsa(key, str_to_sign.as_bytes());

    use base64::prelude::*;
    BASE64_STANDARD.encode(sign.to_bytes())
}

fn sign_sha256_rsa(key: RsaPrivateKey, data: &[u8]) -> rsa::pkcs1v15::Signature {
    let signing_key = SigningKey::<Sha256>::new(key);
    let mut rng = rand::thread_rng();

    signing_key.sign_with_rng(&mut rng, data)
}

fn verify_sha256_rsa(key: RsaPublicKey, data: &[u8], sign: &[u8]) -> Result<(), ()> {
    let verifying_key = VerifyingKey::<Sha256>::new(key);

    let sign = Signature::try_from(sign).map_err(|_| ())?;

    verifying_key.verify(data, &sign).map_err(|_| ())
}

/// 计算“小程序调起支付签名”
/// https://pay.weixin.qq.com/doc/v3/merchant/4012365341
pub fn pay_sign(appid: &str, key: RsaPrivateKey, prepay_id: &str) -> (i64, String, String) {
    let nonce = {
        let mut rng = rand::thread_rng();
        (0..30)
            .map(|_| rng.sample(rand::distributions::Alphanumeric) as char)
            .collect::<String>()
    };

    let timestamp = OffsetDateTime::now_utc().unix_timestamp();

    let sign = pay_sign_inner(key, appid, &nonce, timestamp, prepay_id);

    (timestamp, nonce, sign)
}

fn pay_sign_inner(
    key: RsaPrivateKey,
    app_id: &str,
    nonce: &str,
    timestamp: i64,
    prepay_id: &str,
) -> String {
    let str_to_sign = format!("{app_id}\n{timestamp}\n{nonce}\nprepay_id={prepay_id}\n");

    let sign = sign_sha256_rsa(key, str_to_sign.as_bytes());

    use base64::prelude::*;
    BASE64_STANDARD.encode(sign.to_bytes())
}

pub fn verify_response(
    key: RsaPublicKey,
    sign: &str,
    timestamp: &str,
    nonce: &str,
    body: &[u8],
) -> Result<(), ()> {
    let str_to_sign = format!("{timestamp}\n{nonce}\n{}\n", String::from_utf8_lossy(body));

    use base64::prelude::*;
    let sign = BASE64_STANDARD.decode(sign).map_err(|_| ())?;

    verify_sha256_rsa(key, str_to_sign.as_bytes(), &sign)
}

#[cfg(test)]
mod tests {
    use rsa::{
        pkcs1::{DecodeRsaPrivateKey as _, DecodeRsaPublicKey as _},
        pkcs8::{DecodePrivateKey as _, DecodePublicKey as _},
    };

    use super::*;

    #[test]
    fn get_pay_sign() {
        const TEST_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCm2mb6q8gMKH/3
CNTbpJAIrbqiBiQGEOtjGcBrDYltsGynWgNscqT7WvfzU14FQbYcQUC5T4Wvva7m
i3fIp3OgX8VqMDNA0qebnr38Pe6kqiLyZgFpJPXlSKDyPyqhRbVTbXssvSMQeVKc
dXeVxoNNeoOlNFHgF/P0io6AmAVnz+hN8SiZKuOsth5/zUTLGvtkgxBcQooQrtXh
RcpLT798OyIb9xeJ2HO3xRtMv2+perEzb4gMibI74UBz+2QEbnkubPE+2jU2rRZu
dnNEz/BPOt3Qj/w2V6/G0VumGDh6+UeMU0jv4aupHztWITC4Akn0l7lBCNy3lgl8
VFaJnkIxAgMBAAECggEAYGL8aESB7NwciDWW2UdoWUsa7GxFtSdjAz2mFXGdeTsY
mVh7b9OOkRGM+Qio4LqEHDBp1mMk5E/cUJwy1zw8pGGO5nfvs7u9TT3XnHaefIs4
YvUgTYAneIuLRkXNN5rQU+CD7mVYczTSz0Vgjqo9wa1LjUz7G0xbBmJgTdMEFGJs
eJjy6AbJo0CGIwp6HJbTm4CmOUgXnnDAIbEGTIRImkZFH/rzneIeR7oZ77FVwxr1
CZB2gfRCov/yRPbw8vnryYkmvQ7D/ze3j5097vRg/MoDGBSdoOwcmo75vyofr0AS
zytMjmHYyifqkf5slPropSiJeGf4p/7gtKyF6dE/XQKBgQDVAlJ+4U5ZVGOuDc3+
sAhz8CTzgFNlq9vKuSoFK6hOz2L+cwj+E7NXGkOe2DsHHZNy2Xqxk7caKhPEp1z9
hhpMpyLVMoFt6CKemyoRBWDCQwLLwem9SZF/IAyovBkLiH36P42Jm26gUkNMKC/5
Zhtqxf6RZgRQzbVudJi47vIRCwKBgQDIh0+v27Oo+DM3fhObH4I1NrXpWOEGH7OQ
G1dEsMuFYF4hjGhg0kBEP3w9vVdl2+mRllZKTsx9oqjb8OibPLLIH8xsdbAB0WLf
JvjLu4wl/ILUzN1RI03dWnnv2EnEeQn6c3hizvrJ9wR5U4ue9RPVnQooJ0hZF1PU
uCL5fWK3MwKBgElReU/PAYbh80WP3t3Rfbdaa32dKBeQ5iCLR5lsA4zM+YgX1HqQ
EWTj126vgvHaDkyz6vWAoL/Sx+cirHFfXWIRDX5Q2hgYlQH+6qXdMgbrxeSYpHnQ
/tHBGFpkFELSAnrGsVMyOwvYBO4LzyeLK9i+ufcWJFoj1FVmsMLHDG8tAoGARdbi
iQQCoYG4DMarO2aQ6cmhN6EN1h0qY7EyBqlwaIZ0okiNfdMcMOjPc41DKCWcRmlO
qlihXcxN9TQFPzO3rH1urAOdBjUPs1qWYhZyrDQyuLyVBBJApyxAtajloDjrob+f
mQIvVDHk7ACN6xG+E7K6+9salnTKbJapD618uQMCgYBNy6XUvzLkP/A1U/UZdtcx
l8GwU/dturLxz4CyGbqDw4ubaYY2e13lnqHUqQgPtiSyH51nq3tdo8G0YAJdfkSv
KvnfslW91fyEBUKnkdW1o3/1UFU/wprZ7ixVL/F42A4xDu7OFE8EnweJOZ0jWceE
OdhCkaIGBCfRnlECRK8UyQ==
-----END PRIVATE KEY-----"#;
        const APP_ID: &str = "wx2421b1c4370ec43b";
        const TIMESTAMP: i64 = 1554208460;
        const NONCE: &str = "593BEC0C930BF1AFEB40B4A08C8FB242";
        const PREPAY_ID: &str = "wx201410272009395522657a690389285100";
        const RESULT: &str = "mI35pfNEQV6777ke/1T+LJLQDNTm7yeoUJH+j/adPGhmCCi0PbgkvYQTRcXH0uibcLVtvFLdGLpmoYO9FV6lBBsTAjuhh5YOvQi0e2g3e0yytitiNET9FEuqM0pjnKfRW4K6LIZDdbWJv9KhZUx3DrJa5TL7OJ7VdADVivxVySIlPVKjGwuCXzuXSJes0UcILgWQUMyha5/3nYofuHtS7r+KYyMuxD+oJ9VM1Qdxk4UIG59CP5Y3wtYIFybyF3bdu1caHTRRX+DLyMXyYA/IrTmiW01c4RPjpHBX5Dk1sZyY1zVsWNsvMHr2e1NTWtBxKJ+qk5N61J7caYoepHFaxw==";

        let pem = pem::parse(TEST_PEM).expect("parse test pem");
        let key = match pem.tag() {
            "RSA PRIVATE KEY" => RsaPrivateKey::from_pkcs1_der(pem.contents()).expect("decode key"),
            "PRIVATE KEY" => RsaPrivateKey::from_pkcs8_der(pem.contents()).expect("decode key"),
            _ => panic!("Unsupported key type: {}", pem.tag()),
        };

        let sign = pay_sign_inner(key, APP_ID, NONCE, TIMESTAMP, PREPAY_ID);

        assert_eq!(sign, RESULT);
    }

    #[test]
    fn test_body_sign() {
        const TEST_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQCm2mb6q8gMKH/3
CNTbpJAIrbqiBiQGEOtjGcBrDYltsGynWgNscqT7WvfzU14FQbYcQUC5T4Wvva7m
i3fIp3OgX8VqMDNA0qebnr38Pe6kqiLyZgFpJPXlSKDyPyqhRbVTbXssvSMQeVKc
dXeVxoNNeoOlNFHgF/P0io6AmAVnz+hN8SiZKuOsth5/zUTLGvtkgxBcQooQrtXh
RcpLT798OyIb9xeJ2HO3xRtMv2+perEzb4gMibI74UBz+2QEbnkubPE+2jU2rRZu
dnNEz/BPOt3Qj/w2V6/G0VumGDh6+UeMU0jv4aupHztWITC4Akn0l7lBCNy3lgl8
VFaJnkIxAgMBAAECggEAYGL8aESB7NwciDWW2UdoWUsa7GxFtSdjAz2mFXGdeTsY
mVh7b9OOkRGM+Qio4LqEHDBp1mMk5E/cUJwy1zw8pGGO5nfvs7u9TT3XnHaefIs4
YvUgTYAneIuLRkXNN5rQU+CD7mVYczTSz0Vgjqo9wa1LjUz7G0xbBmJgTdMEFGJs
eJjy6AbJo0CGIwp6HJbTm4CmOUgXnnDAIbEGTIRImkZFH/rzneIeR7oZ77FVwxr1
CZB2gfRCov/yRPbw8vnryYkmvQ7D/ze3j5097vRg/MoDGBSdoOwcmo75vyofr0AS
zytMjmHYyifqkf5slPropSiJeGf4p/7gtKyF6dE/XQKBgQDVAlJ+4U5ZVGOuDc3+
sAhz8CTzgFNlq9vKuSoFK6hOz2L+cwj+E7NXGkOe2DsHHZNy2Xqxk7caKhPEp1z9
hhpMpyLVMoFt6CKemyoRBWDCQwLLwem9SZF/IAyovBkLiH36P42Jm26gUkNMKC/5
Zhtqxf6RZgRQzbVudJi47vIRCwKBgQDIh0+v27Oo+DM3fhObH4I1NrXpWOEGH7OQ
G1dEsMuFYF4hjGhg0kBEP3w9vVdl2+mRllZKTsx9oqjb8OibPLLIH8xsdbAB0WLf
JvjLu4wl/ILUzN1RI03dWnnv2EnEeQn6c3hizvrJ9wR5U4ue9RPVnQooJ0hZF1PU
uCL5fWK3MwKBgElReU/PAYbh80WP3t3Rfbdaa32dKBeQ5iCLR5lsA4zM+YgX1HqQ
EWTj126vgvHaDkyz6vWAoL/Sx+cirHFfXWIRDX5Q2hgYlQH+6qXdMgbrxeSYpHnQ
/tHBGFpkFELSAnrGsVMyOwvYBO4LzyeLK9i+ufcWJFoj1FVmsMLHDG8tAoGARdbi
iQQCoYG4DMarO2aQ6cmhN6EN1h0qY7EyBqlwaIZ0okiNfdMcMOjPc41DKCWcRmlO
qlihXcxN9TQFPzO3rH1urAOdBjUPs1qWYhZyrDQyuLyVBBJApyxAtajloDjrob+f
mQIvVDHk7ACN6xG+E7K6+9salnTKbJapD618uQMCgYBNy6XUvzLkP/A1U/UZdtcx
l8GwU/dturLxz4CyGbqDw4ubaYY2e13lnqHUqQgPtiSyH51nq3tdo8G0YAJdfkSv
KvnfslW91fyEBUKnkdW1o3/1UFU/wprZ7ixVL/F42A4xDu7OFE8EnweJOZ0jWceE
OdhCkaIGBCfRnlECRK8UyQ==
-----END PRIVATE KEY-----"#;
        const TEST_NONCE: &str = "593BEC0C930BF1AFEB40B4A08C8FB242";
        const TEST_TIMESTAMP: i64 = 1554208460;
        const TEST_BODY: &str = r#"{"appid":"wxd678efh567hg6787","mchid":"1900007291","description":"Image形象店-深圳腾大-QQ公仔","out_trade_no":"1217752501201407033233368018","notify_url":"https://www.weixin.qq.com/wxpay/pay.php","amount":{"total":100,"currency":"CNY"},"payer":{"openid":"oUpF8uMuAJO_M2pxb1Q9zNjWeS6o"}}"#;
        const TEST_RESULT: &str = "jnks4dlrPw3ZX+ozVvSK39oa0t7OMBsg83BHAwd8BRdUFiVaQNTLTvci+wURgP1OQBbKYhFGvt7iqYpDSTQkp7Uq1sltaQKyncCyrA1g88m5bsKERQfPyT0ahSwKTYJ1CAn9QiJuSJRq1QsQs07eehbU/k9BCS51jTyc1Jpsi2H77HF9f/BnjXAOP3/sPObg6V5Ee4EzwLox684hhuMuIwHo7D8KFk3LIHOKDcNI4It1aCXydFWNpNK+SG86VUDe5kwoDpw4Ulqfu9z8OFDGbDs9TCxEv8iqQzbpxOlEVoOe2kalSYM5kApQb3nZcxdUtoE0liJGW3RGUNE0t4v01A==";

        let pem = pem::parse(TEST_PEM).expect("parse test pem");
        let key = match pem.tag() {
            "RSA PRIVATE KEY" => RsaPrivateKey::from_pkcs1_der(pem.contents()).expect("decode key"),
            "PRIVATE KEY" => RsaPrivateKey::from_pkcs8_der(pem.contents()).expect("decode key"),
            _ => panic!("Unsupported key type: {}", pem.tag()),
        };

        let sign = get_sign(
            key,
            http::Method::POST,
            "/v3/pay/transactions/jsapi",
            TEST_BODY,
            TEST_NONCE,
            TEST_TIMESTAMP,
        );

        assert_eq!(sign, TEST_RESULT);
    }

    #[test]
    fn verify_response() {
        const PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAwXNI6sdlknHBnK8Fu2U6
Cwor9qY747jP8KAfeBMeveEt1TqaHkLfaSD07trZLhGpfs8/AHqjhgSMO1O10YQW
OrrJ4hjIWPKqxbgrYMkBQc+mwdiWp4W3ByCqxBRagCveCXRWCmuJYovl9H/bsDI0
iGbpVtEOghJtfciisYSgxcLufUDTRkvwxjIBK1pCRjk33jJ5YTBWTHMRtMAOcFLN
F6hdEYdX8SPsgHHeLZ5Lv2T/686w1xtgCHef/sd4uSfWmyzsalQdHG/e4IyYmrhx
+O3VBoNDzE3nx23bFeV/RVNCG7cV6VhmYokJNHa/erIPkEmEFID6A5wQOXuxUkmJ
WwIDAQAB
-----END PUBLIC KEY-----"#;
        const SIGN: &str = "mfI1CPqvBrgcXfgXMFjdNIhBf27ACE2YyeWsWV9ZI7T7RU0vHvbQpu9Z32ogzc+k8ZC5n3kz7h70eWKjgqNdKQF0eRp8mVKlmfzMLBVHbssB9jEZEDXThOX1XFqX7s7ymia1hoHQxQagPGzkdWxtlZPZ4ZPvr1RiqkgAu6Is8MZgXXrRoBKqjmSdrP1N7uxzJ/cjfSiis9FiLjuADoqmQ1P7p2N876YPAol7Rn0+GswwAwxldbdLrmVSjfytfSBJFqTMHn4itojgxSWWN1byuckQt8hSTEv/Lg97QoeGniYP17T80pJeQyL3b+295FPHSO2AtvCgyIbKMZ0BALilAA==";
        const TIMESTAMP: &str = "1722850421";
        const NONCE: &str = "d824f2e086d3c1df967785d13fcd22ef";
        const BODY: &str = r#"{"code_url":"weixin://wxpay/bizpayurl?pr=JyC91EIz1"}"#;

        let pem = pem::parse(PEM).expect("parse wechat pay public pem");

        let key = match pem.tag() {
            "RSA PUBLIC KEY" => {
                RsaPublicKey::from_pkcs1_der(pem.contents()).expect("decode public pem")
            }
            "PUBLIC KEY" => {
                RsaPublicKey::from_public_key_der(pem.contents()).expect("decode public pem")
            }
            _ => panic!("Unsupported pem key type: {}", pem.tag()),
        };

        assert!(super::verify_response(key, SIGN, TIMESTAMP, NONCE, BODY.as_bytes()).is_ok());
    }
}
