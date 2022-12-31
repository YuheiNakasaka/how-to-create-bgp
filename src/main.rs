use how_to_create_bgp::config::Config;
use how_to_create_bgp::peer::Peer;
use std::str::FromStr;

#[tokio::main]
async fn main() {
    let config = vec![Config::from_str("6452 127.0.0.1 65413 127.0.0.2 active").unwrap()];
    let mut peers: Vec<Peer> = config.into_iter().map(Peer::new).collect();
    for peer in &mut peers {
        peer.start();
    }

    let mut handles = vec![];
    for mut peer in peers {
        let handle = tokio::spawn(async move {
            loop {
                peer.next().await;
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await;
    }
}
