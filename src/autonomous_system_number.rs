#[derive(PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Clone, Copy)]
pub struct AutonomousSystemNumber(u16);

impl From<u16> for AutonomousSystemNumber {
    fn from(as_number: u16) -> Self {
        Self(as_number)
    }
}
