use std::net::{IpAddr, Ipv4Addr};
use std::ops::{Deref, DerefMut};
use std::str::FromStr;

use crate::bgp_type::AutonomousSystemNumber;
use crate::config::Config;
use crate::error::ConfigParseError;
use crate::path_attribute::{AsPath, Origin, PathAttribute};
use anyhow::{Context, Result};
use bytes::{BufMut, BytesMut};
use futures::stream::{Next, TryStreamExt};
use rtnetlink::{new_connection, Handle, IpVersion};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, PartialOrd, Ord)]
pub struct Ipv4Network(ipnetwork::Ipv4Network);

impl Deref for Ipv4Network {
    type Target = ipnetwork::Ipv4Network;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Ipv4Network {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<ipnetwork::Ipv4Network> for Ipv4Network {
    fn from(ip_network: ipnetwork::Ipv4Network) -> Self {
        Self(ip_network)
    }
}

impl From<&Ipv4Network> for BytesMut {
    fn from(network: &Ipv4Network) -> BytesMut {
        let prefix = network.prefix();

        let n = network.network().octets();
        let network_bytes = match prefix {
            0 => vec![],
            1..9 => n[0..1].into(),
            9..17 => n[0..2].into(),
            17..25 => n[0..3].into(),
            25..33 => n[0..4].into(),
            _ => panic!("prefixが0..32の間ではありません！"),
        };
        let mut bytes = BytesMut::new();
        bytes.put_u8(prefix);
        bytes.put(&network_bytes[..]);
        bytes
    }
}

impl FromStr for Ipv4Network {
    type Err = ConfigParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let network = s
            .parse::<ipnetwork::Ipv4Network>()
            .context("s: {:?}を、Ipv4Networkにparse出来ませんでした")?;
        Ok(Self(network))
    }
}

impl Ipv4Network {
    pub fn bytes_len(&self) -> usize {
        match self.prefix() {
            0..9 => 2,
            9..17 => 3,
            17..25 => 4,
            25..33 => 5,
            _ => panic!("prefixが0..32の間ではありません！"),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LocRib(Vec<RibEntry>);

impl LocRib {
    pub async fn new(config: &Config) -> Result<Self> {
        let path_attributes = vec![
            PathAttribute::Origin(Origin::Igp),
            // AS Pathは、ほかのピアから受信したルートと統一的に扱うために、
            // LocRib -> AdjRibOutにルートを送るときに、自分のAS番号を
            // 追加するので、ここでは空にしておく。
            PathAttribute::AsPath(AsPath::AsSequence(vec![])),
            PathAttribute::NextHop(config.local_ip),
        ];

        let mut rib = vec![];
        for network in &config.networks {
            let routes = Self::lookup_kernel_routing_table(*network).await?;
            for route in routes {
                rib.push(RibEntry {
                    network_address: route,
                    path_attributes: path_attributes.clone(),
                })
            }
        }
        Ok(Self(rib))
    }

    async fn lookup_kernel_routing_table(
        network_address: Ipv4Network,
    ) -> Result<(Vec<(Ipv4Network)>)> {
        let (connection, handle, _) = new_connection()?;
        tokio::spawn(connection);
        let mut routes = handle.route().get(IpVersion::V4).execute();
        let mut results = vec![];
        while let Some(route) = routes.try_next().await? {
            let destination = if let Some((IpAddr::V4(addr), prefix)) = route.destination_prefix() {
                ipnetwork::Ipv4Network::new(addr, prefix)?.into()
            } else {
                continue;
            };

            if destination != network_address {
                continue;
            }

            results.push((destination));
        }
        Ok(results)
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct AdjRibOut(pub Vec<RibEntry>);

impl AdjRibOut {
    pub fn new() -> Self {
        Self(vec![])
    }

    pub fn install_from_loc_rib(&mut self, loc_rib: &LocRib, config: &Config) {
        for r in &loc_rib.0 {
            let mut route = r.clone();
            route.append_as_path(config.local_as);
            route.change_next_hop(config.local_ip);
            self.0.push(route);
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RibEntry {
    pub network_address: Ipv4Network,
    pub path_attributes: Vec<PathAttribute>,
}

impl RibEntry {
    fn append_as_path(&mut self, as_number: AutonomousSystemNumber) {
        for path_attribute in &mut self.path_attributes {
            if let PathAttribute::AsPath(as_path) = path_attribute {
                as_path.add(as_number)
            };
        }
    }

    fn change_next_hop(&mut self, next_hop: Ipv4Addr) {
        for path_attribute in &mut self.path_attributes {
            if let PathAttribute::NextHop(addr) = path_attribute {
                *addr = next_hop;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn loclib_can_lookup_routing_table() {
        // 本テストの値は環境によって異なる。
        // 本実装では開発機, テスト実施機に192.168.1.0/24に属するIPが付与されていることを仮定している。
        let network = ipnetwork::Ipv4Network::new("10.200.100.0".parse().unwrap(), 24)
            .unwrap()
            .into();
        let routes = LocRib::lookup_kernel_routing_table(network).await.unwrap();
        let expected = vec![network];
        assert_eq!(routes, expected);
    }

    #[tokio::test]
    async fn loc_rib_to_adj_rib_out() {
        // 本テストの値は環境によって異なる。
        // 本実装では開発機, テスト実施機に10.200.100.0/24に属するIPが付与されていることを仮定している。
        // docker-composeした環境のhost2で実行することを仮定している。
        let config: Config = "64513 10.200.100.3 64512 10.200.100.2 passive 10.100.220.0/24"
            .parse()
            .unwrap();
        let mut loc_rib = LocRib::new(&config).await.unwrap();
        let mut adj_rib_out = AdjRibOut::new();
        adj_rib_out.install_from_loc_rib(&mut loc_rib, &config);

        let expected_adj_rib_out = AdjRibOut(vec![RibEntry {
            network_address: "10.100.220.0/24".parse().unwrap(),
            path_attributes: vec![
                PathAttribute::Origin(Origin::Igp),
                PathAttribute::AsPath(AsPath::AsSequence(vec![64513.into()])),
                PathAttribute::NextHop("10.200.100.3".parse().unwrap()),
            ],
        }]);

        assert_eq!(adj_rib_out, expected_adj_rib_out);
    }
}
