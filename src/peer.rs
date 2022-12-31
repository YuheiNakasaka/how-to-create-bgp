use crate::{config::Config, config::Mode, event::Event, event_queue::EventQueue, state::State};
use anyhow::{Context, Result};
use tokio::net::{TcpListener, TcpStream};

#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub struct Peer {
    state: State,
    event_queue: EventQueue,
    config: Config,
}

impl Peer {
    pub fn new(config: Config) -> Self {
        Self {
            state: State::Idle,
            event_queue: EventQueue::new(),
            config,
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
                    self.state = State::Connect;
                }
                _ => {}
            },
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

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
