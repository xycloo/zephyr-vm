use serde::Deserialize;
use sha2::{Digest, Sha256};
use soroban_env_host::storage::{EntryWithLiveUntil, SnapshotSource};
use soroban_env_host::{xdr::LedgerEntry as HostLE, HostError};
use soroban_simulation::SnapshotSourceWithArchive;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::rc::Rc;
use stellar_xdr::{
    Hash, LedgerEntry, LedgerEntryData, LedgerKey, LedgerKeyTtl, Limits, ReadXdr, WriteXdr,
};

/// gets the entry and ttl from core.
pub fn entry_and_ttl(key: Vec<u8>) -> anyhow::Result<Option<(Vec<u8>, Option<u32>)>> {
    configurable_entry_and_ttl(key, "127.0.0.1:8085".to_string())
}

#[test]
fn test_entry() {
    let key = LedgerKey::from_xdr_base64("AAAABgAAAAEW9Ze+ONLuolxPei68E6CdvjqCy0KvFiI77RghqZcOowAAABAAAAABAAAAAgAAAA8AAAAURmVlUGVyU2hhcmVVbml2ZXJzYWwAAAAEAAAAWwAAAAE=", Limits::none()).unwrap();
    println!("{:?}", key);
    let resp = entry_and_ttl(key.to_xdr(Limits::none()).unwrap());
    println!("{:?}", resp);
}

pub fn configurable_entry_and_ttl(
    key: Vec<u8>,
    base_url: String,
) -> anyhow::Result<Option<(Vec<u8>, Option<u32>)>> {
    let key = LedgerKey::from_xdr(key, Limits::none()).unwrap();
    let entry_key = key.to_xdr_base64(Limits::none())?;

    // NB: only soroban entries have ttl entries. classic entries must carry no
    // live_until, the p28 host rejects classic entries paired with a ttl.
    let is_soroban = matches!(
        key,
        LedgerKey::ContractData(_) | LedgerKey::ContractCode(_)
    );

    let resp = if is_soroban {
        let mut hasher = Sha256::new();
        hasher.update(key.to_xdr(Limits::none()).unwrap());
        let hashed = hasher.finalize().as_slice().try_into().unwrap();
        let ttl_key = LedgerKey::Ttl(LedgerKeyTtl {
            key_hash: Hash(hashed),
        })
        .to_xdr_base64(Limits::none())?;
        fetch_ledger_entries_raw(&base_url, &[&entry_key, &ttl_key])?
    } else {
        fetch_ledger_entries_raw(&base_url, &[&entry_key])?
    };

    // NB: core returns only the found entries, ordered by entry type (ttl
    // last), so classify by decoded type instead of position.
    let mut entry = None;
    let mut live_until = None;
    for wrapper in &resp.entries {
        let decoded = LedgerEntry::from_xdr_base64(&wrapper.entry_b64, Limits::none()).unwrap();
        if let LedgerEntryData::Ttl(ttl) = &decoded.data {
            live_until = Some(ttl.live_until_ledger_seq);
        } else {
            entry = Some(decoded);
        }
    }

    let Some(entry) = entry else {
        return Ok(None);
    };
    let ttl_entry = if is_soroban {
        // NB: fall back to max liveness if core did not return the ttl entry.
        Some(live_until.unwrap_or(u32::MAX))
    } else {
        None
    };

    Ok(Some((entry.to_xdr(Limits::none()).unwrap(), ttl_entry)))
}

#[derive(Debug, Deserialize)]
pub struct GetLedgerEntryRawResponse {
    pub entries: Vec<LedgerEntryWrapper>,
    #[serde(rename = "ledgerSeq")]
    pub ledger_seq: u32,
}

#[derive(Debug, Deserialize)]
pub struct LedgerEntryWrapper {
    #[serde(rename = "entry")]
    pub entry_b64: String,
}

fn extract_json(raw_http: &str) -> Option<&str> {
    if let Some(idx) = raw_http.find("\r\n\r\n") {
        return Some(&raw_http[idx + 4..]);
    }
    if let Some(idx) = raw_http.find("\n\n") {
        return Some(&raw_http[idx + 2..]);
    }
    None
}

fn fetch_ledger_entries_raw(
    host_port: &str,
    keys_b64: &[&str],
) -> anyhow::Result<GetLedgerEntryRawResponse> {
    assert!(!keys_b64.is_empty(), "must supply at least one key");

    let mut body = String::new();
    for (i, k) in keys_b64.iter().enumerate() {
        if i > 0 {
            body.push('&');
        }
        body.push_str("key=");
        body.push_str(&urlencoding::encode(k));
    }

    let request = format!(
        "POST /getledgerentryraw HTTP/1.1\r\n\
         Host: {host_port}\r\n\
         User-Agent: curl/8.4.0\r\n\
         Accept: */*\r\n\
         Content-Type: application/x-www-form-urlencoded\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {}",
        body.len(),
        body
    );

    let mut stream = TcpStream::connect(host_port)?;
    stream.write_all(request.as_bytes())?;
    stream.flush()?;

    let mut raw_resp = String::new();
    stream.read_to_string(&mut raw_resp)?;

    println!("{raw_resp}");

    let json = extract_json(&raw_resp)
        .ok_or(anyhow::anyhow!("failed to locate JSON body (no header/body delimiter)"))?
        .to_string();

    let resp: GetLedgerEntryRawResponse = if let Ok(data) = serde_json::from_str(&json) {
        data
    } else {
        tracing::error!("response doesn't contain entries field, likely the entry doesn't exist, requested entries are {:?}", keys_b64);
        return Err(anyhow::anyhow!("invalid json response").into());
    };

    Ok(resp)
}

/// Snapshot that communicates with core's raw entry endpoint.
pub struct DynamicSnapshotFromRaw {}

impl SnapshotSourceWithArchive for DynamicSnapshotFromRaw {
    fn get_including_archived(
        &self,
        key: &std::rc::Rc<LedgerKey>,
    ) -> std::result::Result<
        Option<soroban_env_host::storage::EntryWithLiveUntil>,
        soroban_env_host::HostError,
    > {
        let entry = entry_and_ttl(key.to_xdr(Limits::none()).unwrap())
            .map_err(|_| HostError::from(soroban_env_host::Error::from_contract_error(0)))?
            .map(|(bytes, ttl)| {
                let le = HostLE::from_xdr(bytes, Limits::none()).unwrap();
                (Rc::new(le), ttl)
            });

        Ok(entry)
    }
}

impl SnapshotSource for DynamicSnapshotFromRaw {
    fn get(&self, key: &Rc<LedgerKey>) -> Result<Option<EntryWithLiveUntil>, HostError> {
        let entry = entry_and_ttl(key.to_xdr(Limits::none()).unwrap())
            .map_err(|_| HostError::from(soroban_env_host::Error::from_contract_error(0)))?
            .map(|(bytes, ttl)| {
                let le = HostLE::from_xdr(bytes, Limits::none()).unwrap();
                (Rc::new(le), ttl)
            });

        Ok(entry)
    }
}

#[cfg(test)]
mod test {
    use super::entry_and_ttl;
    use sha2::Digest;
    use stellar_xdr::{
        AccountId, Hash, LedgerEntry, LedgerKey, LedgerKeyAccount, LedgerKeyTtl, Limits, PublicKey,
        ReadXdr, Uint256, WriteXdr,
    };

    #[test]
    fn request_entry() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let account = LedgerKey::Account(LedgerKeyAccount {
            account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(
                stellar_strkey::ed25519::PublicKey::from_string(
                    "GBVNOBJ72WL5UJZ36WMANEDVUG5GPIED7FNLWVP3PSYMN5RKGIMEUGF3",
                )
                .unwrap()
                .0,
            ))),
        });
        entry_and_ttl(account.to_xdr(Limits::none()).unwrap()).unwrap();
    }

    #[test]
    fn get_key() {
        let account = LedgerKey::Account(LedgerKeyAccount {
            account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(
                stellar_strkey::ed25519::PublicKey::from_string(
                    "GBVNOBJ72WL5UJZ36WMANEDVUG5GPIED7FNLWVP3PSYMN5RKGIMEUGF3",
                )
                .unwrap()
                .0,
            ))),
        })
        .to_xdr_base64(Limits::none())
        .unwrap();
        println!("{account}");
    }

    #[test]
    fn get_key_ttl() {
        let account = LedgerKey::Account(LedgerKeyAccount {
            account_id: AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(
                stellar_strkey::ed25519::PublicKey::from_string(
                    "GBVNOBJ72WL5UJZ36WMANEDVUG5GPIED7FNLWVP3PSYMN5RKGIMEUGF3",
                )
                .unwrap()
                .0,
            ))),
        });
        let ttl = LedgerKey::Ttl(LedgerKeyTtl {
            key_hash: Hash(
                sha2::Sha256::digest(account.to_xdr(Limits::none()).unwrap())
                    .try_into()
                    .unwrap(),
            ),
        })
        .to_xdr_base64(Limits::none())
        .unwrap();
        println!("{ttl}");
    }

    #[test]
    fn read_entry() {
        let entry = LedgerEntry::from_xdr_base64("AAfnnwAAAAAAAAAAatcFP9WX2ic79ZgGkHWhumegg/lau1X7fLDG9ioyGEoAAAAXSHboAAAH558AAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAA=", Limits::none()).unwrap();
        println!("{:?}", entry);
    }
}
