use std::ops::RangeInclusive;

// TODO: Use Address
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Address {
    Addr(u16),
    AddrRange(RangeInclusive<u16>),
}
