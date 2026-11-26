use std::error::Error;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tokio::net::UdpSocket;
use tokio::time::Instant;

use crate::torrentsrc;

/// How long one tracker gets to answer.
pub const BUDGET: Duration = Duration::from_secs(4);
/// Probes in flight at once.
const PARALLEL: usize = 32;
/// The most of an HTTP reply that is read.
const BODY_LIMIT: usize = 64 * 1024;
/// What every UDP tracker conversation opens with. BEP 15.
const CONNECT_MAGIC: u64 = 0x0417_2710_1980;

/// What a tracker said when asked whether it is there.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct TrackerHealth {
    pub url: String,
    /// "answered", "silent" or "skipped".
    pub status: &'static str,
    /// The round trip in milliseconds, when it answered.
    pub ms: Option<u32>,
    /// Why it did not answer, or why it was not checked.
    pub reason: Option<String>,
}

impl TrackerHealth {
    fn answered(url: &str, took: Duration) -> Self {
        let ms = took.as_millis().clamp(1, u32::MAX as u128) as u32;
        Self { url: url.into(), status: "answered", ms: Some(ms), reason: None }
    }

    fn silent(url: &str, reason: impl Into<String>) -> Self {
        Self { url: url.into(), status: "silent", ms: None, reason: Some(reason.into()) }
    }

    fn skipped(url: &str, reason: &str) -> Self {
        Self { url: url.into(), status: "skipped", ms: None, reason: Some(reason.into()) }
    }
}

/// Checks every tracker, a few at a time, and reports them in the order given.
/// UDP goes out over `device` when there is one; `http` is expected to be tied
/// to the same interface already.
pub async fn check_all(
    http: reqwest::Client,
    device: Option<String>,
    trackers: Vec<String>,
    budget: Duration,
) -> Vec<TrackerHealth> {
    let gate = Arc::new(tokio::sync::Semaphore::new(PARALLEL));
    let mut probes = tokio::task::JoinSet::new();
    for (index, url) in trackers.iter().cloned().enumerate() {
        let (http, device, gate) = (http.clone(), device.clone(), gate.clone());
        probes.spawn(async move {
            let _slot = gate.acquire().await;
            (index, check_one(&http, device.as_deref(), &url, budget).await)
        });
    }

    let mut results: Vec<TrackerHealth> = trackers
        .iter()
        .map(|url| TrackerHealth::skipped(url, "the check did not finish"))
        .collect();
    // Every probe keeps to its budget, so this only cuts off one that is stuck.
    let rounds = trackers.len().div_ceil(PARALLEL) as u32;
    let give_up = Instant::now() + budget * (rounds + 1);
    while let Ok(Some(done)) = tokio::time::timeout_at(give_up, probes.join_next()).await {
        if let Ok((index, health)) = done {
            results[index] = health;
        }
    }
    results
}

async fn check_one(
    http: &reqwest::Client,
    device: Option<&str>,
    url: &str,
    budget: Duration,
) -> TrackerHealth {
    let scheme = url.split_once("://").map(|(scheme, _)| scheme.to_ascii_lowercase());
    let outcome = match scheme.as_deref() {
        Some("udp") => probe_udp(url, device, budget).await,
        Some("http" | "https") => probe_http(http, url, budget).await,
        Some("ws" | "wss") => {
            return TrackerHealth::skipped(url, "WebSocket trackers are not checked")
        }
        _ => return TrackerHealth::skipped(url, "not a tracker address"),
    };
    match outcome {
        Ok(took) => TrackerHealth::answered(url, took),
        Err(reason) => TrackerHealth::silent(url, reason),
    }
}

/// Opens a UDP tracker conversation and waits for the tracker to accept it.
async fn probe_udp(url: &str, device: Option<&str>, budget: Duration) -> Result<Duration, String> {
    let deadline = Instant::now() + budget;
    let parsed = url::Url::parse(url).map_err(|_| "not a valid address")?;
    let host = parsed.host_str().filter(|h| !h.is_empty()).ok_or("no host in the address")?;
    let port = parsed.port().ok_or("no port in the address")?;

    let peer = resolve(host, port, deadline).await?;
    let local: SocketAddr = match peer {
        SocketAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let socket = UdpSocket::bind(local).await.map_err(|_| "could not open a socket")?;
    bind_to(&socket, device)?;
    socket.connect(peer).await.map_err(|_| "no route to it")?;

    // Asked twice, since one lost packet is not a tracker that has gone.
    let mut asked: Vec<(u32, Instant)> = Vec::with_capacity(2);
    let mut packet = [0u8; 512];
    for patience in [budget * 3 / 8, budget] {
        let transaction = transaction_id();
        socket.send(&connect_request(transaction)).await.map_err(|e| socket_reason(&e))?;
        asked.push((transaction, Instant::now()));
        let until = (Instant::now() + patience).min(deadline);
        loop {
            let Ok(read) = tokio::time::timeout_at(until, socket.recv(&mut packet)).await else {
                break;
            };
            let length = read.map_err(|e| socket_reason(&e))?;
            let Some((transaction, reply)) = read_reply(&packet[..length]) else {
                continue;
            };
            let Some((_, sent)) = asked.iter().find(|(id, _)| *id == transaction) else {
                continue;
            };
            return match reply {
                Ok(()) => Ok(sent.elapsed()),
                Err(said) => Err(format!("replied with an error: {said}")),
            };
        }
    }
    Err("no reply".into())
}

/// One address for the host, IPv4 when it has one.
async fn resolve(host: &str, port: u16, deadline: Instant) -> Result<SocketAddr, String> {
    let lookup = tokio::net::lookup_host(format!("{host}:{port}"));
    let found: Vec<SocketAddr> = tokio::time::timeout_at(deadline, lookup)
        .await
        .map_err(|_| "the name lookup took too long")?
        .map_err(|_| "host not found")?
        .collect();
    let first = found.iter().find(|a| a.is_ipv4()).or(found.first());
    Ok(*first.ok_or("host not found")?)
}

/// Ties the socket to the interface torrent traffic is tied to.
#[cfg(target_os = "linux")]
fn bind_to(socket: &UdpSocket, device: Option<&str>) -> Result<(), String> {
    let Some(name) = device else {
        return Ok(());
    };
    socket
        .bind_device(Some(name.as_bytes()))
        .map_err(|_| format!("the network interface {name} is not available"))
}

/// Only Linux can tie a socket to a device, and only there is one ever set.
#[cfg(not(target_os = "linux"))]
fn bind_to(_socket: &UdpSocket, _device: Option<&str>) -> Result<(), String> {
    Ok(())
}

fn socket_reason(error: &std::io::Error) -> String {
    use std::io::ErrorKind::{ConnectionRefused, ConnectionReset};
    match error.kind() {
        ConnectionRefused | ConnectionReset => "refused the connection".into(),
        _ => "could not be reached".into(),
    }
}

fn connect_request(transaction: u32) -> [u8; 16] {
    let mut packet = [0u8; 16];
    packet[..8].copy_from_slice(&CONNECT_MAGIC.to_be_bytes());
    // Bytes 8 to 12 stay zero, the action that asks to connect.
    packet[12..].copy_from_slice(&transaction.to_be_bytes());
    packet
}

/// The transaction a reply belongs to, and whether it accepted the connection
/// or refused it with a message. None for anything else.
fn read_reply(packet: &[u8]) -> Option<(u32, Result<(), String>)> {
    let action = u32::from_be_bytes(packet.get(..4)?.try_into().ok()?);
    let transaction = u32::from_be_bytes(packet.get(4..8)?.try_into().ok()?);
    match action {
        0 if packet.len() >= 16 => Some((transaction, Ok(()))),
        3 => {
            let said = String::from_utf8_lossy(&packet[8..]).trim().to_string();
            Some((transaction, Err(said)))
        }
        _ => None,
    }
}

fn transaction_id() -> u32 {
    let random = random_id();
    u32::from_be_bytes([random[0], random[1], random[2], random[3]])
}

/// Twenty random bytes, the size of an info hash or a peer ID.
fn random_id() -> [u8; 20] {
    let mut id = [0u8; 20];
    id[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
    id[16..].copy_from_slice(&uuid::Uuid::new_v4().as_bytes()[..4]);
    id
}

/// Announces that a made-up torrent has stopped. Any tracker answers that
/// with a bencoded reply, and it tells the tracker nothing about real ones.
async fn probe_http(
    http: &reqwest::Client,
    url: &str,
    budget: Duration,
) -> Result<Duration, String> {
    let started = Instant::now();
    let request = http.get(announce_url(url)).timeout(budget);
    let mut response = request.send().await.map_err(|e| request_reason(&e))?;
    let status = response.status();

    let mut body = Vec::new();
    while body.len() < BODY_LIMIT {
        match response.chunk().await.map_err(|e| request_reason(&e))? {
            Some(chunk) => body.extend_from_slice(&chunk),
            None => break,
        }
    }
    let took = started.elapsed();

    if is_tracker_reply(&body) {
        Ok(took)
    } else if !status.is_success() {
        Err(format!("replied {status}"))
    } else {
        Err("the reply was not from a tracker".into())
    }
}

fn announce_url(tracker: &str) -> String {
    let base = tracker.split('#').next().unwrap_or(tracker);
    let joiner = if base.contains('?') { '&' } else { '?' };
    format!(
        "{base}{joiner}info_hash={}&peer_id={}&port=6881&uploaded=0&downloaded=0&left=0\
         &compact=1&numwant=0&event=stopped",
        escaped(&random_id()),
        escaped(&random_id()),
    )
}

/// Raw bytes as they go in a query string.
fn escaped(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("%{byte:02X}")).collect()
}

/// A tracker replies with a bencoded dictionary, also when it turns the
/// announce down with a failure reason.
fn is_tracker_reply(body: &[u8]) -> bool {
    torrentsrc::top_level_entries(body.trim_ascii()).is_some()
}

fn request_reason(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        return "no reply".into();
    }
    if error.is_builder() {
        return "not a valid address".into();
    }
    if error.is_redirect() {
        return "too many redirects".into();
    }
    // The causes only, since the error itself quotes the address.
    let mut said = String::new();
    let mut cause = error.source();
    while let Some(error) = cause {
        said.push_str(&error.to_string().to_ascii_lowercase());
        said.push(' ');
        cause = error.source();
    }
    connect_reason(&said).into()
}

/// A plain reason picked out of everything a failed request said.
fn connect_reason(said: &str) -> &'static str {
    let has = |words: &[&str]| words.iter().any(|w| said.contains(w));
    if has(&["dns", "lookup"]) {
        "host not found"
    } else if has(&["certificate", "ssl", "tls", "handshake"]) {
        "the secure connection failed"
    } else if has(&["refused"]) {
        "refused the connection"
    } else {
        "could not connect"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    const QUICK: Duration = Duration::from_millis(400);

    fn client() -> reqwest::Client {
        reqwest::Client::builder().no_proxy().build().unwrap()
    }

    /// A UDP tracker on this machine that answers each packet with `reply`.
    async fn udp_tracker(
        reply: impl Fn(u32) -> Vec<Vec<u8>> + Send + 'static,
    ) -> (String, tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>) {
        let socket = UdpSocket::bind(("127.0.0.1", 0)).await.unwrap();
        let url = format!("udp://{}/announce", socket.local_addr().unwrap());
        let (seen, received) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut packet = [0u8; 64];
            while let Ok((length, from)) = socket.recv_from(&mut packet).await {
                let asked = packet[..length].to_vec();
                let transaction = u32::from_be_bytes(asked[12..16].try_into().unwrap());
                let _ = seen.send(asked);
                for answer in reply(transaction) {
                    let _ = socket.send_to(&answer, from).await;
                }
            }
        });
        (url, received)
    }

    fn accepted(transaction: u32) -> Vec<u8> {
        let mut reply = vec![0u8; 16];
        reply[4..8].copy_from_slice(&transaction.to_be_bytes());
        reply[8..].copy_from_slice(&7u64.to_be_bytes());
        reply
    }

    /// An HTTP tracker on this machine that answers every request with
    /// `response`, and hands back the request lines it was sent.
    async fn http_tracker(
        response: impl Into<String>,
    ) -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let response = response.into();
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let url = format!("http://{}/announce", listener.local_addr().unwrap());
        let (seen, received) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut request = [0u8; 2048];
                let length = socket.read(&mut request).await.unwrap_or(0);
                let text = String::from_utf8_lossy(&request[..length]);
                let _ = seen.send(text.lines().next().unwrap_or_default().to_string());
                let _ = socket.write_all(response.as_bytes()).await;
            }
        });
        (url, received)
    }

    #[test]
    fn a_connect_request_is_laid_out_as_trackers_expect() {
        let packet = connect_request(0x0102_0304);
        assert_eq!(packet[..8], [0, 0, 0x04, 0x17, 0x27, 0x10, 0x19, 0x80]);
        assert_eq!(packet[8..12], [0, 0, 0, 0]);
        assert_eq!(packet[12..], [1, 2, 3, 4]);
    }

    #[test]
    fn replies_are_read_by_action_and_transaction() {
        assert_eq!(read_reply(&accepted(9)), Some((9, Ok(()))));

        let mut refused = vec![0, 0, 0, 3, 0, 0, 0, 9];
        refused.extend_from_slice(b"go away");
        assert_eq!(read_reply(&refused), Some((9, Err("go away".into()))));

        // Too short to be a connect reply, an announce reply, and nothing at all.
        assert_eq!(read_reply(&accepted(9)[..12]), None);
        assert_eq!(read_reply(&[0, 0, 0, 1, 0, 0, 0, 9, 0, 0, 0, 0, 0, 0, 0, 0]), None);
        assert_eq!(read_reply(&[]), None);
    }

    #[test]
    fn only_a_bencoded_dictionary_is_a_tracker_reply() {
        assert!(is_tracker_reply(b"d8:intervali1800e5:peers0:e"));
        assert!(is_tracker_reply(b"d14:failure reason20:unregistered torrente"));
        assert!(is_tracker_reply(b"d8:intervali1800e5:peers0:e\n"));

        assert!(!is_tracker_reply(b"<!doctype html><html><body>For sale</body></html>"));
        assert!(!is_tracker_reply(b"d8:intervali1800e"));
        assert!(!is_tracker_reply(b"l4:spame"));
        assert!(!is_tracker_reply(b"Invalid request"));
        assert!(!is_tracker_reply(b""));
    }

    #[test]
    fn the_announce_keeps_a_passkey_and_names_a_stopped_torrent() {
        let plain = announce_url("http://tracker.example/announce");
        assert!(plain.starts_with("http://tracker.example/announce?info_hash=%"));
        assert!(plain.ends_with("&compact=1&numwant=0&event=stopped"));
        assert!(plain.contains("&left=0&"));

        let keyed = announce_url("http://tracker.example/announce?passkey=abc#x");
        assert!(keyed.starts_with("http://tracker.example/announce?passkey=abc&info_hash=%"));
        assert_ne!(plain, announce_url("http://tracker.example/announce"));
    }

    #[test]
    fn failed_requests_are_given_plain_reasons() {
        let dns = "error sending request dns error: failed to lookup address information";
        assert_eq!(connect_reason(dns), "host not found");
        assert_eq!(
            connect_reason("client error (connect) certificate verify failed"),
            "the secure connection failed"
        );
        assert_eq!(
            connect_reason("tcp connect error connection refused (os error 111)"),
            "refused the connection"
        );
        assert_eq!(connect_reason("network is unreachable"), "could not connect");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_udp_tracker_that_accepts_has_answered() {
        // A stray reply to some other conversation comes first and is ignored.
        let (url, mut seen) =
            udp_tracker(|id| vec![accepted(id.wrapping_add(1)), accepted(id)]).await;

        let health = check_one(&client(), None, &url, QUICK).await;

        assert_eq!((health.status, health.reason), ("answered", None));
        assert!(health.ms.is_some());
        assert_eq!(seen.recv().await.unwrap()[..12], connect_request(0)[..12]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_udp_tracker_that_says_nothing_is_asked_twice() {
        let (url, mut seen) = udp_tracker(|_| Vec::new()).await;

        let started = Instant::now();
        let health = check_one(&client(), None, &url, QUICK).await;

        assert_eq!(health.status, "silent");
        assert_eq!(health.reason.as_deref(), Some("no reply"));
        assert_eq!(health.ms, None);
        assert!(started.elapsed() >= QUICK);
        let (first, second) = (seen.recv().await.unwrap(), seen.recv().await.unwrap());
        assert_ne!(first[12..], second[12..]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_late_reply_to_the_first_packet_still_counts() {
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (url, _seen) = udp_tracker({
            let asked = asked.clone();
            move |id| {
                let mut asked = asked.lock().unwrap();
                asked.push(id);
                // Nothing for the first packet until the second arrives.
                if asked.len() == 2 { vec![accepted(asked[0])] } else { Vec::new() }
            }
        })
        .await;

        let health = check_one(&client(), None, &url, QUICK).await;

        assert_eq!(health.status, "answered");
        assert!(health.ms.unwrap() >= (QUICK * 3 / 8).as_millis() as u32);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_udp_error_reply_is_not_an_answer() {
        let (url, _seen) = udp_tracker(|id| {
            let mut refused = vec![0, 0, 0, 3];
            refused.extend_from_slice(&id.to_be_bytes());
            refused.extend_from_slice(b"closed");
            vec![refused]
        })
        .await;

        let health = check_one(&client(), None, &url, QUICK).await;

        assert_eq!(health.status, "silent");
        assert_eq!(health.reason.as_deref(), Some("replied with an error: closed"));
    }

    /// Skipped on Windows, where this read does not return.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn a_udp_port_with_nothing_on_it_says_so_at_once() {
        let gone = UdpSocket::bind(("127.0.0.1", 0)).await.unwrap().local_addr().unwrap();

        let started = Instant::now();
        let health =
            check_one(&client(), None, &format!("udp://{gone}/announce"), QUICK * 10).await;

        assert_eq!(health.status, "silent");
        assert_eq!(health.reason.as_deref(), Some("refused the connection"));
        assert!(started.elapsed() < QUICK * 10);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_address_that_cannot_be_asked_says_why() {
        let no_port = check_one(&client(), None, "udp://127.0.0.1/announce", QUICK).await;
        assert_eq!(no_port.status, "silent");
        assert_eq!(no_port.reason.as_deref(), Some("no port in the address"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_http_tracker_answers_with_peers_or_a_refusal() {
        for body in ["d8:intervali1800e5:peers0:e", "d14:failure reason7:unknowne"] {
            let response =
                format!("HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{body}", body.len());
            let (url, mut seen) = http_tracker(response).await;

            let health = check_one(&client(), None, &url, QUICK).await;

            assert_eq!((health.status, health.reason), ("answered", None), "{body}");
            let asked = seen.recv().await.unwrap();
            assert!(asked.starts_with("GET /announce?info_hash=%"), "{asked}");
            assert!(asked.contains("&numwant=0&event=stopped "), "{asked}");
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_web_page_is_not_an_answer() {
        let page = "HTTP/1.1 200 OK\r\ncontent-length: 21\r\n\r\n<html>For sale</html>";
        let (url, _seen) = http_tracker(page).await;
        let health = check_one(&client(), None, &url, QUICK).await;
        assert_eq!(health.status, "silent");
        assert_eq!(health.reason.as_deref(), Some("the reply was not from a tracker"));

        let missing = "HTTP/1.1 404 Not Found\r\ncontent-length: 9\r\n\r\nNot found";
        let (url, _seen) = http_tracker(missing).await;
        let health = check_one(&client(), None, &url, QUICK).await;
        assert_eq!(health.status, "silent");
        assert_eq!(health.reason.as_deref(), Some("replied 404 Not Found"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_http_tracker_that_is_not_there_has_not_answered() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let gone = listener.local_addr().unwrap();
        drop(listener);
        // Windows keeps trying a closed port for a while before it says so.
        let refused =
            check_one(&client(), None, &format!("http://{gone}/announce"), BUDGET * 2).await;
        assert_eq!(refused.status, "silent");
        assert_eq!(refused.reason.as_deref(), Some("refused the connection"));

        // Accepts the connection and then says nothing.
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let mute = listener.local_addr().unwrap();
        let slow = check_one(&client(), None, &format!("http://{mute}/announce"), QUICK).await;
        assert_eq!(slow.status, "silent");
        assert_eq!(slow.reason.as_deref(), Some("no reply"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn websocket_trackers_are_left_unchecked() {
        for url in ["ws://tracker.example/announce", "WSS://tracker.example:443/announce"] {
            let health = check_one(&client(), None, url, QUICK).await;
            assert_eq!(health.status, "skipped");
            assert_eq!(health.reason.as_deref(), Some("WebSocket trackers are not checked"));
        }
        let odd = check_one(&client(), None, "tracker.example", QUICK).await;
        assert_eq!(odd.status, "skipped");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn every_tracker_is_reported_in_the_order_given() {
        let (udp, _seen) = udp_tracker(|id| vec![accepted(id)]).await;
        let (quiet, _unseen) = udp_tracker(|_| Vec::new()).await;
        let (http, _asked) =
            http_tracker("HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nde").await;
        let mut trackers = vec![quiet.clone(), "ws://tracker.example/announce".into(), udp, http];
        // More than run at once, so some wait their turn.
        trackers.extend((0..PARALLEL).map(|n| format!("{quiet}?n={n}")));

        let started = Instant::now();
        let results = check_all(client(), None, trackers.clone(), QUICK).await;

        let urls: Vec<&str> = results.iter().map(|r| r.url.as_str()).collect();
        assert_eq!(urls, trackers.iter().map(String::as_str).collect::<Vec<_>>());
        let said: Vec<&str> = results.iter().take(4).map(|r| r.status).collect();
        assert_eq!(said, ["silent", "skipped", "answered", "answered"]);
        assert!(started.elapsed() < QUICK * 4);
    }

    /// The interface sees the fields `TrackerHealth` in `src/lib/types.ts` names.
    #[test]
    fn a_result_reaches_the_interface_in_the_agreed_shape() {
        let answered = TrackerHealth::answered("udp://a:1/announce", Duration::from_millis(84));
        assert_eq!(
            serde_json::to_value(&answered).unwrap(),
            serde_json::json!({
                "url": "udp://a:1/announce", "status": "answered", "ms": 84, "reason": null
            })
        );
        let silent = TrackerHealth::silent("udp://a:1/announce", "no reply");
        assert_eq!(
            serde_json::to_value(&silent).unwrap(),
            serde_json::json!({
                "url": "udp://a:1/announce", "status": "silent", "ms": null, "reason": "no reply"
            })
        );
    }

    /// A check goes out over the bound interface or not at all.
    #[cfg(target_os = "linux")]
    #[tokio::test(flavor = "multi_thread")]
    async fn udp_checks_follow_the_bound_interface() {
        let (url, _seen) = udp_tracker(|id| vec![accepted(id)]).await;

        let through = check_one(&client(), Some("lo"), &url, QUICK).await;
        assert_eq!(through.status, "answered");

        let blocked = check_one(&client(), Some("gh-not-there0"), &url, QUICK).await;
        assert_eq!(blocked.status, "silent");
        assert_eq!(
            blocked.reason.as_deref(),
            Some("the network interface gh-not-there0 is not available")
        );
    }
}
