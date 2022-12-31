use crate::{config::Config, config::Mode, event::Event, event_queue::EventQueue, state::State};
use anyhow::{Context, Result};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug)]
pub struct Peer {
    state: State,
    event_queue: EventQueue,
    tcp_connection: Option<TcpStream>,
    config: Config,
}

impl Peer {
    pub fn new(config: Config) -> Self {
        Self {
            state: State::Idle,
            event_queue: EventQueue::new(),
            config,
            tcp_connection: None,
        }
    }

    pub fn start(&mut self) {
        self.event_queue.enqueue(Event::ManualStart);
    }

    pub async fn next(&mut self) {
        if let Some(event) = self.event_queue.dequeue() {
            self.handle_event(&event).await;
        }
    }

    async fn handle_event(&mut self, event: &Event) {
        match &self.state {
            State::Idle => match event {
                Event::ManualStart => {
                    self.tcp_connection = match self.config.mode {
                        Mode::Active => self.connect_to_remote_peer().await,
                        Mode::Passive => self.wait_connection_from_remote_peer().await,
                    }
                    .ok();
                    self.tcp_connection.as_ref().unwrap_or_else(|| {
                        panic!("Failed to start TCP Connection. {:?}", self.config)
                    });
                    self.state = State::Connect;
                }
                _ => {}
            },
            _ => {}
        }
    }

    async fn connect_to_remote_peer(&self) -> Result<TcpStream> {
        let bgp_port = 179;
        TcpStream::connect((self.config.remote_ip, bgp_port))
            .await
            .context(format!(
                "cannot connect to remote peer {0}:{1}",
                self.config.remote_ip, bgp_port
            ))
    }

    async fn wait_connection_from_remote_peer(&self) -> Result<TcpStream> {
        let bgp_port = 179;
        let listener = TcpListener::bind((self.config.local_ip, bgp_port))
            .await
            .context(format!(
                "cannot bind {0}:{1}",
                self.config.local_ip, bgp_port
            ))?;
        Ok(listener
            .accept()
            .await
            .context(format!(
                "cannot accept {0}:{1}",
                self.config.local_ip, bgp_port
            ))?
            .0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::Duration;

    #[tokio::test]
    async fn peer_can_transition_to_connect_state() {
        // 自分のAS番号 自分のIP 対向側のAS番号 対向側のAS番号動作モード active
        let config: Config = "64512 127.0.0.1 65413 127.0.0.2 active".parse().unwrap();
        let mut peer = Peer::new(config);
        peer.start();

        // 別スレッドでもPeerを立ち上げて対向機器を模擬する
        tokio::spawn(async move {
            let remote_config = "64513 127.0.0.2 65412 127.0.0.1 passive".parse().unwrap();
            let mut remote_peer = Peer::new(remote_config);
            remote_peer.start();
            remote_peer.next().await;
        });

        // 対向機器が起動するまで待つ
        tokio::time::sleep(Duration::from_secs(1)).await;
        peer.next().await;
        assert_eq!(peer.state, State::Connect);
    }
}
