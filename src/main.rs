use how_to_create_bgp::config::Config;
use how_to_create_bgp::peer::Peer;
use std::env;
use std::str::FromStr;

#[tokio::main]
async fn main() {
    let config = env::args().skip(1).fold("".to_owned(), |mut acc, s| {
        acc += &(s.to_owned() + " ");
        acc
    });
    let config = config.trim_end();
    let configs = vec![Config::from_str(&config).unwrap()];

    let mut peers: Vec<Peer> = configs.into_iter().map(Peer::new).collect();
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
