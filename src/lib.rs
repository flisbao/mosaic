#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

#[cfg(test)]
pub(crate) mod test_support;
