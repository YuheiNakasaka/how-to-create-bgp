use crate::{
    config::Config, config::Mode, connection::Connection, event::Event, event_queue::EventQueue,
    packets::message::Message, state::State,
};
use anyhow::{Context, Result};
use tokio::net::{TcpListener, TcpStream};

#[derive(Debug)]
pub struct Peer {
    state: State,
    event_queue: EventQueue,
    tcp_connection: Option<Connection>,
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

        if let Some(conn) = &mut self.tcp_connection {
            if let Some(message) = conn.get_message().await {
                self.handle_message(message);
            }
        }
    }

    fn handle_message(&mut self, message: Message) {
        match message {
            Message::Open(open) => self.event_queue.enqueue(Event::BgpOpen(open)),
            Message::Keepalive(keepalive) => {
                self.event_queue.enqueue(Event::KeepAliveMsg(keepalive))
            }
        }
    }

    async fn handle_event(&mut self, event: &Event) {
        match &self.state {
            State::Idle => match event {
                Event::ManualStart => {
                    self.tcp_connection = Connection::connect(&self.config).await.ok();
                    if self.tcp_connection.is_some() {
                        self.event_queue.enqueue(Event::TcpConnectionConfirmed);
                    } else {
                        panic!("Failed to start TCP Connection. {:?}", self.config)
                    }
                    self.state = State::Connect;
                }
                _ => {}
            },
            State::Connect => match event {
                Event::TcpConnectionConfirmed => {
                    self.tcp_connection
                        .as_mut()
                        .unwrap()
                        .send(Message::new_open(
                            self.config.local_as,
                            self.config.local_ip,
                        ))
                        .await;
                    self.state = State::OpenSent
                }
                _ => {}
            },
            State::OpenSent => match event {
                Event::BgpOpen(open) => {
                    self.tcp_connection
                        .as_mut()
                        .unwrap()
                        .send(Message::new_keepalive())
                        .await;
                    self.state = State::OpenConfirm;
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

    #[tokio::test]
    async fn peer_can_transition_to_open_sent_state() {
        let config: Config = "64512 127.0.0.1 65413 127.0.0.2 active".parse().unwrap();
        let mut peer = Peer::new(config);
        peer.start();

        tokio::spawn(async move {
            let remote_config = "64513 127.0.0.2 65412 127.0.0.1 passive".parse().unwrap();
            let mut remote_peer = Peer::new(remote_config);
            remote_peer.start();
            remote_peer.next().await;
            remote_peer.next().await;
        });

        tokio::time::sleep(Duration::from_secs(1)).await;
        peer.next().await;
        peer.next().await;
        assert_eq!(peer.state, State::OpenSent);
    }

    #[tokio::test]
    async fn peer_can_transition_to_open_confirm_state() {
        let config: Config = "64512 127.0.0.1 65413 127.0.0.2 active".parse().unwrap();
        let mut peer = Peer::new(config);
        peer.start();

        tokio::spawn(async move {
            let remote_config = "64513 127.0.0.2 65412 127.0.0.1 passive".parse().unwrap();
            let mut remote_peer = Peer::new(remote_config);
            remote_peer.start();
            let max_step = 50;
            for _ in 0..max_step {
                remote_peer.next().await;
                if remote_peer.state == State::OpenConfirm {
                    break;
                };
                tokio::time::sleep(Duration::from_secs_f32(0.1)).await;
            }
        });

        tokio::time::sleep(Duration::from_secs(1)).await;
        let max_step = 50;
        for _ in 0..max_step {
            peer.next().await;
            if peer.state == State::OpenConfirm {
                break;
            };
            tokio::time::sleep(Duration::from_secs_f32(0.1)).await;
        }
        assert_eq!(peer.state, State::OpenConfirm);
    }

    #[tokio::test]
    async fn peer_can_transition_to_established_state() {
        let config: Config = "64512 127.0.0.1 65413 127.0.0.2 active".parse().unwrap();
        let mut peer = Peer::new(config);
        peer.start();

        // 別スレッドでPeer構造体を実行しています。
        // これはネットワーク上で離れた別のマシンを模擬しています。
        tokio::spawn(async move {
            let remote_config = "64513 127.0.0.2 65412 127.0.0.1 passive".parse().unwrap();
            let mut remote_peer = Peer::new(remote_config);
            remote_peer.start();
            let max_step = 50;
            for _ in 0..max_step {
                remote_peer.next().await;
                if remote_peer.state == State::Established {
                    break;
                };
                tokio::time::sleep(Duration::from_secs_f32(0.1)).await;
            }
        });

        // 先にremote_peer側の処理が進むことを保証するためのwait
        tokio::time::sleep(Duration::from_secs(1)).await;
        let max_step = 50;
        for _ in 0..max_step {
            peer.next().await;
            if peer.state == State::Established {
                break;
            };
            tokio::time::sleep(Duration::from_secs_f32(0.1)).await;
        }
        assert_eq!(peer.state, State::Established);
    }
}
