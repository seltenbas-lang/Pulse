//! download_to writes a song as it arrives and reports each piece (a trickling local server).
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

#[tokio::test(flavor = "multi_thread")]
async fn writes_as_it_arrives() {
    let body: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let served = body.clone();
    std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut req = [0u8; 2048];
        let _ = s.read(&mut req);
        write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", served.len()).unwrap();
        for c in served.chunks(30_000) {
            s.write_all(c).unwrap();
            s.flush().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
    });
    let net = pulse_net::http::Net::new().unwrap();
    let audio = pulse_net::Audio {
        stream: pulse_net::Stream::File { url: format!("http://127.0.0.1:{port}/song.m4a"), size: None },
        user_agent: "test".into(),
        ext: "m4a".into(),
        title: String::new(),
        artist: String::new(),
        duration: 0.0,
    };
    let path = std::env::temp_dir().join("pulse-net-download-to.part");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    pulse_net::download_to(&net, &audio, &path, move |w, total| s2.lock().unwrap().push((w, total)))
        .await
        .unwrap();
    let seen = seen.lock().unwrap().clone();
    assert!(seen.len() > 3, "progress came in pieces: {seen:?}");
    assert!(seen.windows(2).all(|w| w[0].0 < w[1].0), "and only grew");
    assert_eq!(seen.last().unwrap(), &(300_000, Some(300_000)));
    assert_eq!(std::fs::read(&path).unwrap(), body);
}
